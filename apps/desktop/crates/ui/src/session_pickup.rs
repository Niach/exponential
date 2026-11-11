//! EXP-1294 — the desktop's half of "the database is the bus":
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
    /// #EXP-1412: [`stranded_rows`] ran for this app run.
    boot_swept: bool,
    /// #EXP-1492: a re-claim retry pass is armed ([`retry_reclaim`]).
    reclaim_retry: bool,
    /// #LOGS-11: the hosted sessions' newest message-group revision the
    /// last messages delta planned at ([`hosted_messages_moved`]).
    hosted_messages_at: u64,
    /// #LOGS-11: the move records' fingerprint the last events delta saw
    /// ([`move_events_moved`]).
    move_events_at: u64,
}

impl Global for PickupHost {}

#[cfg(test)]
thread_local! {
    /// #LOGS-11: the rail bench plans a Local desktop's sessions as an
    /// account's, to time the pass.
    pub(crate) static PASS_ON_LOCAL: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

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
        boot_swept: false,
        reclaim_retry: false,
        hosted_messages_at: 0,
        move_events_at: 0,
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
    // #LOGS-11: a message only this host's sessions read plans again (a busy
    // team's every other message re-cloned every hosted chat).
    cx.observe(&collections.messages, |_, cx| {
        if hosted_messages_moved(cx) {
            schedule(cx);
        }
    })
    .detach();
    // EXP-1542: a move's record (and its hand-off) re-plans.
    // #LOGS-11: only a move's own events (the stream's every other line
    // planned a whole pass).
    cx.observe(&collections.events, |_, cx| {
        if move_events_moved(cx) {
            schedule(cx);
        }
    })
    .detach();
    let local = LocalSessions::global(cx);
    cx.observe(&local, |_, cx| schedule(cx)).detach();
    schedule(cx);
    // EXP-1458: a direct (eNet) message runs a pass at once.
    let (arrived, arrivals) = flume::unbounded::<()>();
    coding::direct_inbox::on_arrival(Box::new(move || {
        let _ = arrived.send(());
    }));
    cx.spawn(async move |cx| {
        while arrivals.recv_async().await.is_ok() {
            cx.update(schedule);
        }
    })
    .detach();
    // S-4: the daemon's idle release — a quiescent chat gives its agent
    // process back (the row idles, the worktree stays, the next message
    // resumes it). Off the UI thread; a release re-plans.
    let live = Arc::clone(&cx.global::<PickupHost>().live);
    cx.spawn(async move |cx| loop {
        cx.background_executor().timer(IDLE_RELEASE_BEAT).await;
        let live = Arc::clone(&live);
        let released = cx
            .background_executor()
            .spawn(async move { live.release_idle(std::time::Instant::now(), pickup::IDLE_RELEASE) })
            .await;
        if released.is_empty() {
            continue;
        }
        for session_id in &released {
            log::info!("[pickup] released session {session_id}'s agent process (idle)");
        }
        cx.update(schedule);
    })
    .detach();
}

/// S-4: how often the IDE checks for quiescent chats to release.
const IDLE_RELEASE_BEAT: Duration = Duration::from_secs(60);

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
/// #LOGS-11: whether a message of a session this host runs (its device's
/// rows, its live engines) changed since the last messages delta asked.
fn hosted_messages_moved(cx: &mut App) -> bool {
    let Some(live) = cx.try_global::<PickupHost>().map(|host| host.live.session_ids().into_iter().collect::<HashSet<String>>()) else {
        return true;
    };
    let device_id = queries::own_device_id(cx);
    let Some(store) = Store::try_global(cx) else {
        return true;
    };
    let at = {
        let collections = store.collections();
        let messages = collections.messages.read(cx);
        collections
            .sessions
            .read(cx)
            .iter()
            .filter(|row| row.device_id.as_deref() == Some(device_id.as_str()) || live.contains(&row.id))
            .map(|row| row.id.as_str())
            .chain(live.iter().map(String::as_str))
            .map(|id| messages.group_revision(id))
            .max()
            .unwrap_or(0)
    };
    let host = cx.global_mut::<PickupHost>();
    let moved = at != host.hosted_messages_at;
    host.hosted_messages_at = at;
    moved
}

/// #LOGS-11: whether the synced move / hand-off events
/// ([`pickup::move_records`]' input) changed since the last events delta.
fn move_events_moved(cx: &mut App) -> bool {
    use std::hash::{Hash as _, Hasher as _};
    let (Some(store), true) = (Store::try_global(cx), cx.has_global::<PickupHost>()) else {
        return true;
    };
    let at = {
        let events = store.collections().events.read(cx);
        let mut ids: Vec<&str> = events
            .iter()
            .filter(|event| matches!(event.name.as_deref(), Some(pickup::SESSION_MOVED_EVENT | pickup::SESSION_HANDED_OFF_EVENT)))
            .map(|event| event.id.as_str())
            .collect();
        ids.sort_unstable();
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        ids.hash(&mut hasher);
        hasher.finish()
    };
    let host = cx.global_mut::<PickupHost>();
    let moved = at != host.move_events_at;
    host.move_events_at = at;
    moved
}

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
    // #EXP-1727: a Local desktop's sessions are `elocal::runs`'s to claim.
    #[cfg(test)]
    let local_runs = !PASS_ON_LOCAL.with(std::cell::Cell::get);
    #[cfg(not(test))]
    let local_runs = true;
    if queries::active_account(cx).is_none_or(|a| a.is_local() && local_runs) {
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
    // EXP-1458: who an engine reply's `@email` names (`mentionUserIds`).
    coding::sealing::note_users(
        collections.users.read(cx).iter().filter_map(|user| Some((user.id.clone(), user.email.clone()?))),
    );
    // #EXP-1730: keyed by their own `session_id`.
    // #LOGS-11: only the rows a pass can act on (was every hosted message,
    // a long chat's thousands cloned per pass): an unconsumed input, a card,
    // a system line (a move's), a row an engine here holds, a row the direct
    // inbox holds (its dedupe). A consumed plain message is history.
    let held = live.held_message_ids();
    let direct: HashSet<String> = coding::direct_inbox::pending().into_iter().map(|row| row.id).collect();
    let mut messages: Vec<MessageRow> = crate::session_data::keyed_messages(cx, hosted.iter().copied(), |_, message| {
        message.consumed_at.as_deref().is_none_or(str::is_empty)
            || message.card.as_ref().is_some_and(|card| !card.is_null())
            || message.author_kind.as_deref() == Some("system")
            || held.contains(&message.id)
            || direct.contains(&message.id)
    });
    // EXP-1458: agent-to-agent messages that came over eNet ahead of their
    // synced copy (deduped by id against it).
    coding::direct_inbox::merge(&mut messages);
    for session_id in live.prune() {
        log::debug!("[pickup] session {session_id}'s engine is gone");
    }
    live.settle_handed(&messages);
    live.settle_options(&sessions);
    // #LOGS-11: the messages grouped once (was a scan of every hosted
    // message per hosted session).
    let mut by_session: HashMap<&str, Vec<&MessageRow>> = HashMap::new();
    for message in &messages {
        by_session.entry(message.session_id.as_str()).or_default().push(message);
    }
    let fingerprints: HashMap<&str, String> = sessions
        .iter()
        .map(|row| (row.id.as_str(), fingerprint(row, by_session.get(row.id.as_str()).into_iter().flatten().copied())))
        .collect();
    // A stall lasts only while nothing about the session moved.
    cx.global_mut::<PickupHost>()
        .stalled
        .retain(|session_id, at| fingerprints.get(session_id.as_str()) == Some(&*at));
    // #EXP-1412: once per app run, as the daemon's reconcile does: a start
    // that died before its spawn left no registry entry to name it.
    if !cx.global::<PickupHost>().boot_swept && collections.sessions.read(cx).is_ready() {
        let host = cx.global_mut::<PickupHost>();
        host.boot_swept = true;
        crate::session_registry::note_left_behind(stranded_rows(&device_id, &sessions, &live_ids, &host.in_flight));
    }
    // NEW-5: the sessions an earlier process left behind, re-claimed first
    // (a continuation goes in flight before this pass plans).
    reclaim_left_behind(&device_id, &sessions, &messages, &live, &fingerprints, cx);
    let in_flight = cx.global::<PickupHost>().in_flight.clone();
    let stalled = cx.global::<PickupHost>().stalled.clone();
    let views = live.live_views();
    let revive_held = live.revive_held();
    // EXP-1542: the server's move records (a moved session's hand-off).
    // #LOGS-11: only the move lines are cloned (was every synced event).
    let moves = pickup::move_records(
        &collections
            .events
            .read(cx)
            .iter()
            .filter(|event| matches!(event.name.as_deref(), Some(pickup::SESSION_MOVED_EVENT | pickup::SESSION_HANDED_OFF_EVENT)))
            .cloned()
            .collect::<Vec<_>>(),
    );
    let mut actions: Vec<PickupAction> = pickup::plan(PickupInput {
        device_id: &device_id,
        sessions: &sessions,
        messages: &messages,
        live: &views,
        in_flight: &in_flight,
        revive_held: &revive_held,
        moves: &moves,
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
            // #EXP-1492: a claim another process here holds retries on the
            // timer (that process may be on its way out).
            PickupAction::Revive { session } => match pickup::revive_session(session, &deps) {
                Ok(()) => StepOutcome::Progressed,
                Err(pickup::WakeError::Contended) => StepOutcome::Failed,
                Err(err) => {
                    log::warn!("[pickup] reviving {session_id}: {err}");
                    StepOutcome::Progressed
                }
            },
            // EXP-1374: the registry backs a failed continuation off on its
            // own (`revive_held`), like a revive.
            PickupAction::Continue { session } => match pickup::continue_interrupted(session, &deps) {
                Ok(()) => StepOutcome::Progressed,
                Err(pickup::WakeError::Contended) => StepOutcome::Failed,
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

/// #EXP-1412: the rows of this device that read `running` (or idle but `busy`)
/// with no engine here and nothing starting them, the set the daemon's
/// `reconcile_rows` takes back at startup.
fn stranded_rows(
    device_id: &str,
    sessions: &[SessionRow],
    live: &HashSet<String>,
    in_flight: &HashSet<String>,
) -> Vec<String> {
    sessions
        .iter()
        .filter(|row| {
            row.on_device(device_id)
                && (row.status_is(domain::contract::SESSION_STATUS_RUNNING)
                    || (row.status_is(domain::contract::SESSION_STATUS_IDLE) && row.busy))
                && !live.contains(&row.id)
                && !in_flight.contains(&row.id)
        })
        .map(|row| row.id.clone())
        .collect()
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
    let mut contended = false;
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
            contended = true;
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
    // #EXP-1492: passes run on row changes, and a sibling's claim going
    // stale is none: without this re-pass the grace ran out with the
    // session re-claimed by nobody.
    if contended {
        retry_reclaim(cx);
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

/// #EXP-1492: one pass [`RECLAIM_RETRY`] from now for the left-behind
/// sessions a sibling process still held (armed once at a time).
fn retry_reclaim(cx: &mut App) {
    let host = cx.global_mut::<PickupHost>();
    if host.reclaim_retry {
        return;
    }
    host.reclaim_retry = true;
    cx.spawn(async move |cx| {
        cx.background_executor().timer(RECLAIM_RETRY).await;
        let _ = cx.update(|cx| {
            cx.global_mut::<PickupHost>().reclaim_retry = false;
            schedule(cx);
        });
    })
    .detach();
}

/// #EXP-1492: how soon a skipped re-claim tries again (the app before a
/// self-update exits within seconds).
const RECLAIM_RETRY: Duration = Duration::from_secs(5);

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
    let at = fingerprint(&row, None::<&MessageRow>);
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
fn fingerprint<'a>(row: &SessionRow, messages: impl IntoIterator<Item = &'a MessageRow>) -> String {
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

    /// #EXP-1412: a claimed row nothing here runs is stranded; a row with an
    /// engine, a start in flight, between turns or on another device is not.
    #[test]
    fn a_running_row_with_nothing_behind_it_is_stranded() {
        let row = |id: &str, status: &str, busy: bool, device: &str| SessionRow {
            id: id.to_string(),
            status: Some(status.to_string()),
            busy,
            device_id: Some(device.to_string()),
            ..SessionRow::default()
        };
        let rows = vec![
            row("claimed", "running", false, "dev"),
            row("cut", "idle", true, "dev"),
            row("waiting", "idle", false, "dev"),
            row("queued", "queued", false, "dev"),
            row("ended", "ended", true, "dev"),
            row("live", "running", true, "dev"),
            row("starting", "running", false, "dev"),
            row("moved", "running", false, "other"),
        ];
        let live: HashSet<String> = HashSet::from(["live".to_string()]);
        let in_flight: HashSet<String> = HashSet::from(["starting".to_string()]);
        assert_eq!(stranded_rows("dev", &rows, &live, &in_flight), vec!["claimed".to_string(), "cut".to_string()]);
    }

    #[test]
    fn the_fingerprint_moves_with_the_row_and_its_queue() {
        let row: SessionRow = serde_json::from_value(serde_json::json!({
            "id": "s-1", "status": "queued", "updated_at": "t1",
        }))
        .unwrap();
        let before = fingerprint(&row, None::<&MessageRow>);
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
        assert_ne!(before, fingerprint(&later, None::<&MessageRow>));
    }

    /// EXP-1329: another session's message plans a Resume too, so it moves
    /// the fingerprint a stalled Resume waits on — a consumed one does not.
    #[test]
    fn the_fingerprint_moves_with_another_sessions_pending_input() {
        let row: SessionRow = serde_json::from_value(serde_json::json!({
            "id": "s-1", "status": "idle", "updated_at": "t1",
        }))
        .unwrap();
        let before = fingerprint(&row, None::<&MessageRow>);
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
