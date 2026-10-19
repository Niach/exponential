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
    // device now (a move: released without a row write).
    let live_ids: HashSet<String> = live.session_ids().into_iter().collect();
    let all_sessions: Vec<SessionRow> = collections.sessions.read(cx).iter().cloned().collect();
    let sessions: Vec<SessionRow> = all_sessions
        .iter()
        .filter(|row| {
            row.device_id.as_deref() == Some(device_id.as_str()) || live_ids.contains(&row.id)
        })
        .cloned()
        .collect();
    let hosted: HashSet<&str> = sessions.iter().map(|row| row.id.as_str()).collect();
    let messages: Vec<MessageRow> = collections
        .messages
        .read(cx)
        .iter()
        .filter(|row| hosted.contains(row.session_id.as_str()))
        .cloned()
        .collect();
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
    // EXP-1252: sessions handed off from here whose target claimed them.
    actions.extend(
        pickup::cleanup_actions(&coding_flow::coding_data_dir(cx), &device_id, &all_sessions)
            .into_iter()
            .filter(|action| !in_flight.contains(action.session_id())),
    );
    let blocking = |action: &PickupAction| match action {
        // EXP-849: a switch suspends + relaunches (network + git).
        // EXP-1341: a revive resumes/launches (network + git).
        PickupAction::Launch { .. }
        | PickupAction::Resume { .. }
        | PickupAction::Revive { .. }
        | PickupAction::Switch { .. }
        // EXP-1252: a move packs + uploads; a cleanup runs git.
        | PickupAction::Handoff { .. }
        | PickupAction::Cleanup { .. } => true,
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
            PickupAction::Resume { session, message } => woke(
                &session_id,
                pickup::resume_session(session, std::slice::from_ref(message), &deps),
            ),
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
            PickupAction::Switch { session, account, messages } => switched(
                &session_id,
                account,
                pickup::switch_session(session, account, None, messages, &deps),
            ),
            // EXP-1252: a move whose engine is not truly idle yet looks
            // again on the retry timer (the 30 min bound is the registry's).
            PickupAction::Handoff { session } => match pickup::hand_off(session, &deps) {
                Ok(pickup::HandedOff::Waiting) => {
                    log::info!("[pickup] move of {session_id} waits for a quiet moment");
                    StepOutcome::Failed
                }
                Ok(outcome) => {
                    log::info!("[pickup] move of {session_id}: {outcome:?}");
                    StepOutcome::Progressed
                }
                Err(err) => {
                    log::warn!("[pickup] move of {session_id} failed: {err}");
                    StepOutcome::Failed
                }
            },
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
/// to an engine? Nothing handed = a stall (another process holds the
/// session, or the wake was refused for these messages).
fn woke(session_id: &str, result: Result<Vec<String>, String>) -> StepOutcome {
    match result {
        Ok(delivered) if delivered.is_empty() => {
            log::info!("[pickup] nothing delivered to {session_id}");
            StepOutcome::Stalled
        }
        Ok(delivered) => {
            log::info!("[pickup] delivered {} message(s) to {session_id}", delivered.len());
            StepOutcome::Progressed
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
    /// It failed: wait for the row to move, or [`STEP_RETRY`].
    Failed,
}

/// How long a FAILED step waits before the same step is planned again.
const STEP_RETRY: Duration = Duration::from_secs(30);

/// What a stall waits on: the row (its `updated_at` and status) plus the ids
/// of its queued messages — any change re-opens the session to the plan.
fn fingerprint(row: &SessionRow, messages: &[MessageRow]) -> String {
    let mut at = format!(
        "{}|{}",
        row.updated_at.as_deref().unwrap_or_default(),
        row.status.as_deref().unwrap_or_default()
    );
    for message in messages {
        if message.session_id == row.id && message.is_queued() {
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
    // mode (the daemon's rule: an unattended start never parks unless it
    // asked to).
    let mut options = coding::LaunchOptions::defaults_for(&settings, settings.default_agent);
    options.plan_mode = false;
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
        let message: MessageRow = serde_json::from_value(serde_json::json!({
            "id": "m-1", "session_id": "s-1", "root_id": "s-1", "author_kind": "user",
        }))
        .unwrap();
        assert_ne!(before, fingerprint(&row, &[message]));
        let mut later = row.clone();
        later.updated_at = Some("t2".into());
        assert_ne!(before, fingerprint(&later, &[]));
    }
}
