//! EXP-1294 (Threads v2) — the desktop's half of "the database is the bus":
//! this machine hosts every `sessions` row whose `device_id` is its own, and
//! picks queued rows and new person messages up from the synced shapes.
//!
//! The decisions and their execution are `coding::pickup`'s, exactly as the
//! CLI daemon drives them (`daemon::pickup_pass`): this host snapshots the
//! `sessions` and `messages` collections, asks [`coding::pickup::plan`] (plus
//! the registry's [`pickup::LiveRegistry::retry_launches`]) what to do, and
//! runs each step:
//!
//! - `Launch` / `Resume` (and a `Deliver` to an engine that is not live) →
//!   [`coding::pickup::launch_queued`] / [`coding::pickup::resume_session`] /
//!   [`coding::pickup::deliver_messages`] on the background executor; their
//!   `LaunchFn` hops back to the foreground and starts the engine through
//!   [`crate::coding_flow::host_prepared`].
//! - everything else → [`coding::pickup::execute`] straight away (it only
//!   touches the live engine).
//!
//! The engine's own worker consumes delivered messages and reports back, so
//! nothing here writes a row.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use gpui::{App, Global};

use coding::pickup::{self, LaunchQueued, LiveEngine, PickupAction, PickupDeps, PickupInput};
use domain::rows::{MessageRow, SessionRow};
use sync::Store;

use crate::coding_flow::{self, LocalSessions};
use crate::queries;

/// One `LaunchFn` call handed to the foreground: the prepared launch and
/// where its answer goes.
type LaunchRequest = (
    coding::PreparedLaunch,
    flume::Sender<Result<Arc<dyn LiveEngine>, String>>,
);

/// The host's foreground state.
struct PickupHost {
    /// The foreground end of every `LaunchFn` (see [`launch_fn`]).
    launches: flume::Sender<LaunchRequest>,
    /// The pickup's registry of the sessions live here, one for the
    /// process (every pass hands the same one to `coding::pickup`).
    live: Arc<pickup::LiveRegistry>,
    /// Sessions whose launch or delivery is on the background executor.
    in_flight: HashSet<String>,
    /// Sessions whose last step changed nothing (a refused or foreign-held
    /// claim, a failed launch or resume), keyed to the [`fingerprint`] they
    /// stalled at: the plan's `Launch`/`Resume` for them waits until the row
    /// or its queue moves (or the retry timer of a failure clears it).
    /// Without it every finished step re-plans the same step at once.
    stalled: HashMap<String, String>,
    /// A pass is already scheduled for the next effect cycle.
    scheduled: bool,
}

impl Global for PickupHost {}

/// Stand the host up (once, from [`crate::steer_wiring::install`], after the
/// `Store` exists): the launch inbox plus the collection watches that
/// schedule a pass.
pub(crate) fn install(cx: &mut App) {
    if cx.has_global::<PickupHost>() {
        return;
    }
    let (launches, inbox) = flume::unbounded::<LaunchRequest>();
    let live = Arc::new(pickup::LiveRegistry::default());
    install_answer_tap(&live, cx);
    cx.set_global(PickupHost {
        launches,
        live,
        in_flight: HashSet::new(),
        stalled: HashMap::new(),
        scheduled: false,
    });
    cx.spawn(async move |cx| {
        while let Ok((prepared, reply)) = inbox.recv_async().await {
            let hosted = cx.update(|cx| coding_flow::host_prepared(prepared, cx));
            let _ = reply.send(
                hosted.map(|session| -> Arc<dyn LiveEngine> { Arc::new(session) }),
            );
        }
    })
    .detach();
    let Some(collections) = Store::try_global(cx).map(|store| store.collections().clone()) else {
        return;
    };
    cx.observe(&collections.sessions, |_, cx| schedule(cx)).detach();
    cx.observe(&collections.messages, |_, cx| schedule(cx)).detach();
    // NEW-8: a message's session is its conversation's.
    cx.observe(&collections.conversations, |_, cx| schedule(cx)).detach();
    let local = LocalSessions::global(cx);
    cx.observe(&local, |_, cx| schedule(cx)).detach();
    schedule(cx);
}

/// EXP-1316: a card answered in the session view (relay → this engine)
/// closes its synced `messages` card on every client: the publisher's tap
/// maps the question onto the card at once (before the engine resolves it),
/// the foreground writes `sessions.resolveCard` with the active account.
fn install_answer_tap(live: &Arc<pickup::LiveRegistry>, cx: &mut App) {
    let (resolved, inbox) = flume::unbounded::<api::sessions::AnswerInput>();
    let registry = Arc::clone(live);
    steer::answer_tap::add_answer_tap(Arc::new(move |session_id, resolution| {
        if let Some(mut input) = registry.resolve_card_input(
            session_id,
            &resolution.question_id,
            resolution.answer.clone(),
            resolution.decision.clone(),
            resolution.fresh,
            resolution.feedback.clone(),
        ) {
            // EXP-1280: a multi-select's picks ride `answers` too — the
            // closed card highlights off them.
            input.answers = resolution.answers.as_ref().map(|labels| serde_json::json!(labels));
            let _ = resolved.send(input);
        }
    }));
    cx.spawn(async move |cx| {
        while let Ok(input) = inbox.recv_async().await {
            cx.update(|cx| match queries::trpc_client(cx) {
                Some(trpc) => pickup::write_back_card(Arc::new(trpc), input),
                None => log::warn!("pickup: no account to resolve card {}", input.message_id),
            });
        }
    })
    .detach();
}

/// Run one pass on the next effect cycle (coalesced).
fn schedule(cx: &mut App) {
    let Some(host) = cx.try_global::<PickupHost>() else {
        return;
    };
    if host.scheduled {
        return;
    }
    cx.global_mut::<PickupHost>().scheduled = true;
    cx.defer(|cx| {
        cx.global_mut::<PickupHost>().scheduled = false;
        run_pass(cx);
    });
}

/// The session a pickup action is about.
fn action_session_id(action: &PickupAction) -> &str {
    action.session_id()
}

/// One pass: snapshot, plan, execute (the daemon's `pickup_pass`).
fn run_pass(cx: &mut App) {
    if queries::active_account(cx).is_none() {
        return;
    }
    let Some(store) = Store::try_global(cx) else {
        return;
    };
    let collections = store.collections().clone();
    let device_id = queries::own_device_id(cx);
    let live = Arc::clone(&cx.global::<PickupHost>().live);
    // Only this machine's sessions (and their messages) reach the plan —
    // plus, EXP-1252, the rows of engines live here that name another
    // device now (an agent moved in Settings: released without a row write).
    let live_ids: HashSet<String> = live.session_ids().into_iter().collect();
    let sessions: Vec<SessionRow> = collections
        .sessions
        .read(cx)
        .iter()
        .filter(|row| {
            row.device_id.as_deref() == Some(device_id.as_str()) || live_ids.contains(&row.id)
        })
        .cloned()
        .collect();
    let hosted: HashSet<&str> = sessions.iter().map(|row| row.id.as_str()).collect();
    // NEW-8: keyed through their conversation (`session_id` = resolved).
    let messages: Vec<MessageRow> =
        crate::session_data::keyed_messages(cx, |owner, _| hosted.contains(owner));
    for session_id in live.prune() {
        log::debug!("[pickup] session {session_id}'s engine is gone");
    }
    live.settle_handed(&messages);
    live.settle_options(&sessions);
    let fingerprints: HashMap<&str, String> = sessions
        .iter()
        .map(|row| (row.id.as_str(), fingerprint(row, &messages)))
        .collect();
    // A stall lasts only while nothing about the session moved.
    cx.global_mut::<PickupHost>()
        .stalled
        .retain(|session_id, at| fingerprints.get(session_id.as_str()) == Some(&*at));
    // NEW-5: the sessions an earlier process left behind, re-claimed first
    // (a continuation goes in flight before this pass plans).
    reclaim_left_behind(&device_id, &sessions, &messages, &live, &fingerprints, cx);
    let in_flight = cx.global::<PickupHost>().in_flight.clone();
    let stalled = cx.global::<PickupHost>().stalled.clone();
    let views = live.live_views();
    let revive_held = live.revive_held();
    let mut actions: Vec<PickupAction> = pickup::plan(PickupInput {
        device_id: &device_id,
        sessions: &sessions,
        messages: &messages,
        live: &views,
        in_flight: &in_flight,
        revive_held: &revive_held,
    });
    // Rows this host claimed whose launch failed transiently read `running`
    // — the plan only launches `queued` ones.
    actions.extend(live.retry_launches(&device_id, &sessions, &in_flight));
    let blocking = |action: &PickupAction| match action {
        // EXP-849: a switch suspends + relaunches (network + git).
        // EXP-1341: a revive resumes/launches (network + git).
        // EXP-1374: so does an interrupted run's continuation.
        PickupAction::Launch { .. }
        | PickupAction::Resume { .. }
        | PickupAction::Revive { .. }
        | PickupAction::Continue { .. }
        | PickupAction::Switch { .. } => true,
        // A delivery to an engine that just finished wakes it.
        PickupAction::Deliver { session_id, .. } => !live.contains_live(session_id),
        _ => false,
    };
    let actions: Vec<PickupAction> = actions
        .into_iter()
        .filter(|action| !(blocking(action) && stalled.contains_key(action_session_id(action))))
        .collect();
    if actions.is_empty() {
        return;
    }
    let Some(deps) = pickup_deps(&device_id, &sessions, cx) else {
        return;
    };
    let deps = Arc::new(deps);
    for action in actions {
        if !blocking(&action) {
            if let Err(err) = pickup::execute(&action, &deps) {
                log::warn!("[pickup] {err}");
            }
            continue;
        }
        let session_id = action_session_id(&action).to_string();
        if cx.global::<PickupHost>().in_flight.contains(&session_id) {
            continue;
        }
        let at = fingerprints.get(session_id.as_str()).cloned().unwrap_or_default();
        let deps = Arc::clone(&deps);
        spawn_step(session_id.clone(), at, cx, move || match &action {
            PickupAction::Launch { session } => match pickup::launch_queued(session, &deps) {
                Ok(LaunchQueued::Launched { session_id }) => {
                    log::info!("[pickup] launched {session_id}");
                    StepOutcome::Progressed
                }
                // Refused + reported on the row: nothing to retry.
                Ok(LaunchQueued::Disabled { reason }) => {
                    log::warn!("[pickup] {session_id} disabled: {reason}");
                    StepOutcome::Progressed
                }
                Ok(LaunchQueued::Skipped { reason }) => {
                    log::info!("[pickup] {session_id} skipped: {reason}");
                    StepOutcome::Stalled
                }
                Err(err) => {
                    log::warn!("[pickup] launching {session_id} failed: {err}");
                    StepOutcome::Failed
                }
            },
            PickupAction::Resume { session, messages } => {
                woke(&session_id, pickup::resume_session(session, messages, &deps))
            }
            PickupAction::Deliver { messages, .. } => {
                woke(&session_id, pickup::deliver_messages(&session_id, messages, &deps))
            }
            // EXP-1341: the registry backs a failed revive off on its own.
            PickupAction::Revive { session } => match pickup::revive_session(session, &deps) {
                Ok(()) => StepOutcome::Progressed,
                Err(err) => {
                    log::warn!("[pickup] reviving {session_id}: {err}");
                    StepOutcome::Progressed
                }
            },
            // EXP-1374: the registry backs a failed continuation off on its
            // own (`revive_held`), like a revive.
            PickupAction::Continue { session } => match pickup::continue_interrupted(session, &deps) {
                Ok(()) => StepOutcome::Progressed,
                Err(err) => {
                    log::warn!("[pickup] continuing {session_id}: {err}");
                    StepOutcome::Progressed
                }
            },
            PickupAction::Switch { session, account, messages } => switched(
                &session_id,
                account,
                pickup::switch_session(session, account, None, messages, &deps),
            ),
            other => match pickup::execute(other, &deps) {
                Ok(()) => StepOutcome::Progressed,
                Err(err) => {
                    log::warn!("[pickup] {err}");
                    StepOutcome::Failed
                }
            },
        });
    }
}

/// NEW-5: the sessions the pickup holds — engines live here and steps in
/// flight (the heartbeat's `liveSessionIds`).
pub(crate) fn hosted_ids(cx: &App) -> Vec<String> {
    let Some(host) = cx.try_global::<PickupHost>() else {
        return Vec::new();
    };
    let mut ids = host.live.session_ids();
    ids.extend(host.in_flight.iter().cloned());
    ids
}

/// NEW-5 (the daemon's `reconcile_rows`): re-claim each session an earlier
/// process of this app left behind ([`crate::session_registry::left_behind`])
/// once its row is here — a pending input replays through [`pickup::plan`]'s
/// Resume, a mid-turn row resumes with [`pickup::continue_after_restart`]'s
/// continuation turn, a `running` row between turns goes idle. A row a
/// sibling process holds is skipped by its claim lock (EXP-229).
fn reclaim_left_behind(
    device_id: &str,
    sessions: &[SessionRow],
    messages: &[MessageRow],
    live: &pickup::LiveRegistry,
    fingerprints: &HashMap<&str, String>,
    cx: &mut App,
) {
    let left = crate::session_registry::left_behind();
    if left.is_empty() {
        return;
    }
    let data_dir = coding_flow::coding_data_dir(cx);
    let pending: HashSet<&str> = messages
        .iter()
        .filter(|message| pickup::is_pending_input(message))
        .map(|message| message.session_id.as_str())
        .collect();
    let mut mid_turn = Vec::new();
    let mut idle = Vec::new();
    let mut unstarted = Vec::new();
    for row in sessions.iter().filter(|row| left.contains(&row.id)) {
        let open = row.status_is(domain::contract::SESSION_STATUS_RUNNING)
            || row.status_is(domain::contract::SESSION_STATUS_IDLE);
        if !open
            || row.device_id.as_deref() != Some(device_id)
            || live.contains_live(&row.id)
            || cx.global::<PickupHost>().in_flight.contains(&row.id)
        {
            crate::session_registry::settle_left_behind(&row.id);
            continue;
        }
        // The claim a sibling process still holds stays on the list: the
        // process that left the row behind may be the one on its way out
        // (the app before a self-update), its claim stale a few seconds
        // later — the next pass within the grace re-claims it then.
        if !matches!(pickup::ClaimLock::try_acquire(&data_dir, &row.id), Ok(Some(_))) {
            log::info!("[pickup] reclaim: session {} runs in another process here — skipped", row.id);
            continue;
        }
        crate::session_registry::settle_left_behind(&row.id);
        let recorded = coding::pickup::reclaim_recorded(&data_dir, &row.id);
        match pickup::reclaim_step(row, pending.contains(row.id.as_str()), recorded) {
            pickup::ReclaimStep::Replay => {
                log::info!("[pickup] reclaim: session {} re-claimed — its pending messages replay", row.id);
            }
            pickup::ReclaimStep::Continue => mid_turn.push(row.clone()),
            pickup::ReclaimStep::Idle => {
                log::info!("[pickup] reclaim: session {} re-claimed between turns — idle", row.id);
                idle.push(row.id.clone());
            }
            pickup::ReclaimStep::Waiting => {
                log::info!("[pickup] reclaim: session {} re-claimed — idle, waiting for its next message", row.id);
            }
            pickup::ReclaimStep::Unstarted => unstarted.push(row.id.clone()),
        }
    }
    let Some(trpc) = queries::trpc_client(cx).map(Arc::new) else {
        return;
    };
    if !idle.is_empty() || !unstarted.is_empty() {
        let trpc = Arc::clone(&trpc);
        let device_id = device_id.to_string();
        std::thread::spawn(move || {
            for session_id in unstarted {
                pickup::settle_unstarted_on(&trpc, &session_id, Some(&device_id));
            }
            for session_id in idle {
                if let Err(err) = api::session_device::report(&trpc, &session_id, &idle_report(&device_id)) {
                    log::warn!("[pickup] reclaim: session {session_id} not reported idle: {err}");
                }
            }
        });
    }
    if mid_turn.is_empty() {
        return;
    }
    let Some(deps) = pickup_deps(device_id, sessions, cx) else {
        return;
    };
    let deps = Arc::new(deps);
    for row in mid_turn {
        let at = fingerprints.get(row.id.as_str()).cloned().unwrap_or_default();
        let deps = Arc::clone(&deps);
        let trpc = Arc::clone(&trpc);
        let device_id = device_id.to_string();
        spawn_step(row.id.clone(), at, cx, move || {
            let id = row.id.as_str();
            match pickup::continue_after_restart(&row, &deps) {
                Ok(true) => {
                    log::info!("[pickup] reclaim: session {id} was mid-turn — resumed with a continuation turn");
                    StepOutcome::Progressed
                }
                Ok(false) => {
                    match api::session_device::report(&trpc, id, &idle_report(&device_id)) {
                        Ok(()) => log::info!(
                            "[pickup] reclaim: session {id} was mid-turn with no run record here — now idle"
                        ),
                        Err(err) => log::warn!("[pickup] reclaim: session {id} not reported idle: {err}"),
                    }
                    StepOutcome::Progressed
                }
                Err(err) => {
                    log::warn!("[pickup] reclaim: session {id} not continued: {err}");
                    StepOutcome::Failed
                }
            }
        });
    }
}

/// NEW-5: a re-claimed session between turns — idle and not working.
/// Stamped with this host's device id so the server's stale-host guard
/// ignores a copy of a session that has since moved (#EXP-1389).
fn idle_report(device_id: &str) -> api::session_device::SessionReport {
    api::session_device::SessionReport {
        status: Some(api::session_device::ReportStatus::Idle),
        busy: Some(false),
        device_id: Some(device_id.to_string()),
        ..api::session_device::SessionReport::default()
    }
}

/// EXP-1005 — the account ROTATION's switch of a session hosted here: THE
/// switch ([`pickup::switch_session`], the one a person's `sessions.send
/// {sessionId, account}` takes through the pass), run with the session in
/// flight and `prompt` (the rotation's note) as the seed turn. `false` =
/// refused before anything moved: the session is not hosted here, a step
/// already holds it, or its turn is in flight.
pub(crate) fn switch_locally(
    session_id: &str,
    target: String,
    prompt: String,
    cx: &mut App,
) -> bool {
    let Some(host) = cx.try_global::<PickupHost>() else {
        return false;
    };
    if host.in_flight.contains(session_id) {
        return false;
    }
    let live = Arc::clone(&host.live);
    if !live.get(session_id).is_some_and(|engine| !engine.is_done() && engine.is_idle()) {
        return false;
    }
    let Some(row) = crate::session_data::session(session_id, cx) else {
        return false;
    };
    let device_id = queries::own_device_id(cx);
    let Some(deps) = pickup_deps(&device_id, std::slice::from_ref(&row), cx) else {
        return false;
    };
    let at = fingerprint(&row, &[]);
    let id = session_id.to_string();
    spawn_step(id.clone(), at, cx, move || {
        switched(&id, &target, pickup::switch_session(&row, &target, Some(&prompt), &[], &deps))
    });
    true
}

/// Did a wake (a Resume, or a delivery that had to re-spawn) hand anything
/// to an engine? Nothing handed = a stall (the wake was refused for these
/// messages). A claim another process of this install holds is RETRIED on
/// the timer, never parked on the row: that process may be on its way out
/// (the app that just self-updated) and its claim goes stale seconds later
/// with nothing on the row to re-open the plan.
fn woke(session_id: &str, result: Result<Vec<String>, pickup::WakeError>) -> StepOutcome {
    match result {
        Ok(delivered) if delivered.is_empty() => {
            log::info!("[pickup] nothing delivered to {session_id}");
            StepOutcome::Stalled
        }
        Ok(delivered) => {
            log::info!("[pickup] delivered {} message(s) to {session_id}", delivered.len());
            StepOutcome::Progressed
        }
        Err(pickup::WakeError::Contended) => {
            log::info!(
                "[pickup] {session_id} is held by another process here — retrying in {}s",
                STEP_RETRY.as_secs()
            );
            StepOutcome::Failed
        }
        Err(err) => {
            log::warn!("[pickup] delivering to {session_id} failed: {err}");
            StepOutcome::Failed
        }
    }
}

/// How a switch ended: a busy engine waits for its turn to end (the row's
/// `busy` edge moves its fingerprint), a failure for the retry timer.
/// `account` empty = a reconfigure (EXP-1334), named so in the log.
fn switched(
    session_id: &str,
    account: &str,
    result: Result<pickup::Switched, pickup::SwitchError>,
) -> StepOutcome {
    let kind = if account.is_empty() { "reconfigure" } else { "account switch" };
    match result {
        Ok(outcome) => {
            log::info!("[pickup] {kind} of {session_id}: {outcome:?}");
            StepOutcome::Progressed
        }
        Err(pickup::SwitchError::Busy) => StepOutcome::Stalled,
        Err(err) => {
            log::warn!("[pickup] {kind} of {session_id} failed: {err}");
            StepOutcome::Failed
        }
    }
}

/// How a pickup step ended: whether the next pass may plan the same step
/// for that session straight away.
enum StepOutcome {
    /// It moved the session along (the rows will follow).
    Progressed,
    /// It changed nothing and retrying unchanged would change nothing either
    /// (a refused or foreign-held claim): wait for the row to move.
    Stalled,
    /// It failed, or another process here holds the claim: wait for the
    /// row to move, or [`STEP_RETRY`].
    Failed,
}

/// How long a FAILED step waits before the same step is planned again.
const STEP_RETRY: Duration = Duration::from_secs(30);

/// What a stall waits on: the row (its `updated_at` and status) plus the ids
/// of its pending inputs (a person's queued messages AND another session's,
/// EXP-1329: both plan a Resume) — any change re-opens the session to the
/// plan.
fn fingerprint(row: &SessionRow, messages: &[MessageRow]) -> String {
    let mut at = format!(
        "{}|{}",
        row.updated_at.as_deref().unwrap_or_default(),
        row.status.as_deref().unwrap_or_default()
    );
    for message in messages {
        if message.session_id == row.id && pickup::is_pending_input(message) {
            at.push('|');
            at.push_str(&message.id);
        }
    }
    at
}

/// Run one blocking pickup step on the background executor with
/// `session_id` marked in flight, then plan again. A step that changed
/// nothing parks the session at `at` (its [`fingerprint`] when planned).
fn spawn_step(
    session_id: String,
    at: String,
    cx: &mut App,
    step: impl FnOnce() -> StepOutcome + Send + 'static,
) {
    cx.global_mut::<PickupHost>()
        .in_flight
        .insert(session_id.clone());
    cx.spawn(async move |cx| {
        let outcome = cx.background_executor().spawn(async move { step() }).await;
        let failed = matches!(outcome, StepOutcome::Failed);
        let _ = cx.update(|cx| {
            let host = cx.global_mut::<PickupHost>();
            host.in_flight.remove(&session_id);
            if !matches!(outcome, StepOutcome::Progressed) {
                host.stalled.insert(session_id.clone(), at.clone());
            }
            schedule(cx);
        });
        if failed {
            cx.background_executor().timer(STEP_RETRY).await;
            let _ = cx.update(|cx| {
                let host = cx.global_mut::<PickupHost>();
                if host.stalled.get(&session_id) == Some(&at) {
                    host.stalled.remove(&session_id);
                    schedule(cx);
                }
            });
        }
    })
    .detach();
}

/// What the pickup needs from this app, snapshotted on the foreground. The
/// root → identifiers map is captured whole: the pickup asks it on the
/// background executor, where the collections are unreachable.
fn pickup_deps(device_id: &str, sessions: &[SessionRow], cx: &mut App) -> Option<PickupDeps> {
    let coding = coding_flow::build_coding_deps(cx)?;
    let settings = coding.settings.clone();
    // The host defaults under every row's pickers; the row decides plan
    // mode (S-3: the defaults never carry one).
    let options = coding::LaunchOptions::defaults_for(&settings, settings.default_agent);
    let mut identifiers: HashMap<String, Vec<String>> = HashMap::new();
    for row in sessions {
        let root_id = row.root_id_or_self().to_string();
        if !identifiers.contains_key(&root_id) {
            let idents = crate::session_data::root_issue_identifiers(&root_id, cx);
            identifiers.insert(root_id, idents);
        }
    }
    Some(PickupDeps {
        coding: Arc::new(coding),
        device_id: device_id.to_string(),
        device_label: coding::default_device_label(),
        options,
        live: Arc::clone(&cx.global::<PickupHost>().live),
        launch: launch_fn(cx),
        issue_identifiers: Arc::new(move |root_id: &str| {
            identifiers.get(root_id).cloned().unwrap_or_default()
        }),
    })
}

/// The pickup's spawn: called on the background executor, it hands the
/// prepared launch to the foreground inbox ([`install`]) and waits for the
/// engine. Never call it on the foreground — it blocks until the foreground
/// answers.
fn launch_fn(cx: &App) -> pickup::LaunchFn {
    let launches = cx.global::<PickupHost>().launches.clone();
    Arc::new(move |prepared| {
        let (reply, answer) = flume::bounded(1);
        launches
            .send((prepared, reply))
            .map_err(|_| "The app is shutting down.".to_string())?;
        answer
            .recv()
            .map_err(|_| "The app is shutting down.".to_string())?
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_fingerprint_moves_with_the_row_and_its_queue() {
        let row: SessionRow = serde_json::from_value(serde_json::json!({
            "id": "s-1", "status": "queued", "updated_at": "t1",
        }))
        .unwrap();
        let before = fingerprint(&row, &[]);
        let message = MessageRow {
            session_id: "s-1".into(),
            ..serde_json::from_value(serde_json::json!({
                "id": "m-1", "conversation_id": "c-1", "root_id": "s-1", "author_kind": "user",
            }))
            .unwrap()
        };
        assert_ne!(before, fingerprint(&row, &[message]));
        let mut later = row.clone();
        later.updated_at = Some("t2".into());
        assert_ne!(before, fingerprint(&later, &[]));
    }

    /// EXP-1329: another session's message plans a Resume too, so it moves
    /// the fingerprint a stalled Resume waits on — a consumed one does not.
    #[test]
    fn the_fingerprint_moves_with_another_sessions_pending_input() {
        let row: SessionRow = serde_json::from_value(serde_json::json!({
            "id": "s-1", "status": "idle", "updated_at": "t1",
        }))
        .unwrap();
        let before = fingerprint(&row, &[]);
        let from_agent = MessageRow {
            session_id: "s-1".into(),
            ..serde_json::from_value(serde_json::json!({
                "id": "m-2", "conversation_id": "c-1", "root_id": "s-1", "author_kind": "agent",
                "author_session_id": "s-9",
            }))
            .unwrap()
        };
        assert_ne!(before, fingerprint(&row, &[from_agent.clone()]));
        let consumed = MessageRow { consumed_at: Some("t2".into()), ..from_agent };
        assert_eq!(before, fingerprint(&row, &[consumed]));
    }
}
