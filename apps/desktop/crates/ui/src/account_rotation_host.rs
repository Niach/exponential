//! EXP-1005 — the desktop's half of ACCOUNT ROTATION on a usage wall.
//!
//! Every [`BEAT`] this host reads each live local session's wall
//! (`EngineSession::blocked`) and its turn slot, asks the ONE
//! [`coding::account_rotation::RotationTracker`] what to do, and — on a
//! probe — spends the FORCED usage read of every profile of the agent OFF the
//! foreground ([`coding::agent_usage::collect_now`]), then either switches
//! the run (the same resume-on-another-account a person's "Switch account"
//! makes, [`crate::account_switch::end_then_resume_on_account`], with the
//! rotation's own line as the resume's first message) or parks it until the
//! reset. Every guard lives in `coding::account_rotation`; this file is the
//! wiring. The CLI daemon runs the same beat headlessly.
//!
//! EXP-1107: ONE probe per AGENT per beat (`RotationTracker::plan_beat`,
//! `InflightProbes`), every walled run of that agent decided off it under one
//! tracker lock (`decide_batch`); a switch the host could not make is undone
//! (`undo_switch`).
//!
//! The beat itself touches no disk and no network: a walled session's run
//! record (its account, model and whether it is repo-backed) is read ONCE
//! off the foreground into a small cache, the chains are persisted off the
//! foreground, and the workflow event a decision owes goes out on the
//! background executor.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use coding::account_rotation::{Decision, Hold, InflightProbes, ProbeBatch, RotationTracker, WalledRun};
use coding::workflows::events::{TrpcEventSink, WorkflowEventSink as _};
use gpui::App;

use crate::coding_flow::{CodingHub, LocalSessions};

/// How often the walls are read. The switch waits for the turn slot anyway,
/// so a finer beat would buy nothing.
const BEAT: Duration = Duration::from_secs(5);

struct RotationState {
    tracker: Mutex<RotationTracker>,
    /// The agents with a probe batch (and its switches) in flight: never
    /// probed twice, their chains kept in the tracker while a switch has
    /// ended the old run but not yet registered the new one.
    inflight: Mutex<InflightProbes>,
    /// Which hold each walled session was last logged under, so a hold is
    /// logged once per change instead of every beat.
    holds: Mutex<HashMap<String, &'static str>>,
    /// Each walled session's run record (`None` = this host has none), read
    /// once in the background: the registry is a locked file the beat must
    /// not open on the foreground.
    records: Mutex<HashMap<String, Option<coding::run_registry::RunRecord>>>,
    /// Session ids whose record read is in flight.
    reading: Mutex<HashSet<String>>,
}

struct RotationGlobal(Arc<RotationState>);

impl gpui::Global for RotationGlobal {}

/// Start the beat once per process (every later call is a no-op).
pub fn start_account_rotation_host(cx: &mut App) {
    if cx.has_global::<RotationGlobal>() {
        return;
    }
    let state = Arc::new(RotationState {
        tracker: Mutex::new(RotationTracker::new()),
        inflight: Mutex::new(InflightProbes::new()),
        holds: Mutex::new(HashMap::new()),
        records: Mutex::new(HashMap::new()),
        reading: Mutex::new(HashSet::new()),
    });
    cx.set_global(RotationGlobal(Arc::clone(&state)));
    let data_dir = crate::coding_flow::coding_data_dir(cx);
    cx.spawn(async move |cx| {
        // The chains as the last run of this host (or the daemon) left
        // them: a restart must not hand a thrashing chain a fresh cap.
        let loaded = cx
            .background_executor()
            .spawn(async move {
                RotationTracker::load(&data_dir, chrono::Utc::now().timestamp_millis())
            })
            .await;
        *lock(&state.tracker) = loaded;
        loop {
            cx.background_executor().timer(BEAT).await;
            cx.update(|cx| beat(&state, cx));
        }
    })
    .detach();
}

/// One live session's wall in the rotation's vocabulary — `None` unless the
/// wall is a rate limit. A record with no clone (a scratch run), or no
/// record at all, is a run a switch could not resume.
fn walled_run(
    session_id: &str,
    worktree: &std::path::Path,
    agent: coding::CodingAgent,
    blocked: &steer::SessionBlocked,
    record: Option<&coding::run_registry::RunRecord>,
    idle: bool,
) -> Option<WalledRun> {
    (blocked.kind == steer::activity::BLOCKED_KIND_RATE_LIMIT).then(|| WalledRun {
        session_id: session_id.to_string(),
        chain_key: worktree.to_string_lossy().into_owned(),
        agent,
        account: coding::profile_id(record.and_then(|record| record.account()).as_deref()),
        model: record
            .map(|record| record.model.trim().to_string())
            .filter(|model| !model.is_empty()),
        window: blocked.window.clone(),
        resets_at_ms: blocked
            .resets_at
            .as_deref()
            .and_then(coding::agent_accounts::unix_millis_from_iso),
        idle,
        repo_less: record.map_or(true, |record| record.clone.is_none()),
    })
}

/// The log-once key of a hold: its variant, never its timestamps.
fn hold_key(hold: &Hold) -> &'static str {
    match hold {
        Hold::Off => "off",
        Hold::AgentNeverRotates => "agent_never_rotates",
        Hold::ScratchRun => "scratch_run",
        Hold::MidTurn => "mid_turn",
        Hold::CoolingDown { .. } => "cooling_down",
        Hold::Capped => "capped",
        Hold::WaitingForReset { .. } => "waiting_for_reset",
        Hold::RetryingSwitch { .. } => "retrying_switch",
    }
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    match mutex.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

/// The workflow a synced run belongs to: `(workflow_id, node_id)`.
fn session_workflow(session_id: &str, cx: &App) -> Option<(String, Option<String>)> {
    let store = sync::Store::try_global(cx)?;
    let sessions = store.collections().coding_sessions.read(cx);
    sessions
        .iter()
        .find(|row| row.id == session_id)
        .and_then(|row| Some((row.workflow_id.clone()?, row.workflow_node_id.clone())))
}

/// One walled session as the beat reads it off the engine, before its run
/// record is known.
struct Wall {
    session_id: String,
    worktree: std::path::PathBuf,
    agent: coding::CodingAgent,
    blocked: steer::SessionBlocked,
    idle: bool,
}

fn beat(state: &Arc<RotationState>, cx: &mut App) {
    let Some(sessions) = LocalSessions::global_ref(cx) else {
        return;
    };
    let data_dir = crate::coding_flow::coding_data_dir(cx);
    // Every live session (its chain stays in the tracker) and the walled ones.
    let (live_chains, live_ids, walls): (Vec<String>, HashSet<String>, Vec<Wall>) = {
        let sessions = sessions.read(cx);
        let mut chains = Vec::new();
        let mut ids = HashSet::new();
        let mut walls = Vec::new();
        for session_id in sessions.session_ids() {
            let Some(session) = sessions.session_for_id(&session_id) else {
                continue;
            };
            chains.push(session.worktree.to_string_lossy().into_owned());
            ids.insert(session_id.clone());
            let engine = &session.host.session;
            if engine.is_done() {
                continue;
            }
            let Some(blocked) = engine.blocked() else {
                continue;
            };
            walls.push(Wall {
                session_id: session_id.clone(),
                worktree: session.worktree.clone(),
                agent: session.agent,
                blocked,
                idle: engine.turn_signal().is_idle(),
            });
        }
        (chains, ids, walls)
    };
    let now_ms = chrono::Utc::now().timestamp_millis();
    let inflight = lock(&state.inflight).clone();
    let mut keep = live_chains;
    keep.extend(inflight.chains());
    // Grace-based: a chain between an ended row and its resumed successor
    // keeps its cooldown and cap (see `CHAIN_GRACE_MS`).
    lock(&state.tracker).retain_chains(&keep, now_ms);
    lock(&state.records).retain(|session_id, _| live_ids.contains(session_id));
    {
        let walled: HashSet<String> = walls.iter().map(|wall| wall.session_id.clone()).collect();
        lock(&state.holds).retain(|session_id, _| walled.contains(session_id));
    }
    if walls.is_empty() {
        return;
    }
    // The run records the walls need, read once each off the foreground; a
    // wall whose record is not in yet is decided next beat.
    let mut candidates: Vec<WalledRun> = Vec::new();
    for wall in walls {
        let record = lock(&state.records).get(&wall.session_id).cloned();
        let Some(record) = record else {
            read_record(Arc::clone(state), wall.session_id.clone(), data_dir.clone(), cx);
            continue;
        };
        if let Some(run) = walled_run(
            &wall.session_id,
            &wall.worktree,
            wall.agent,
            &wall.blocked,
            record.as_ref(),
            wall.idle,
        ) {
            candidates.push(run);
        }
    }
    if candidates.is_empty() {
        return;
    }
    let (settings, report) = {
        let hub = CodingHub::global(cx);
        let hub = hub.read(cx);
        (hub.settings.clone(), hub.doctor.report.clone())
    };
    // An agent whose batch is in flight is decided again once it lands.
    candidates.retain(|run| !inflight.busy(run.agent));
    let plan = lock(&state.tracker).plan_beat(candidates, settings.auto_rotate_accounts, now_ms);
    for (run, hold) in plan.holds {
        let key = hold_key(&hold);
        if lock(&state.holds).insert(run.session_id.clone(), key) != Some(key) {
            log::info!(
                "account rotation [{}]: walled on {} ({}), holding: {hold:?}",
                run.session_id,
                run.account,
                run.window
            );
        }
    }
    for batch in plan.probes {
        let Some(report) = report.clone() else {
            log::info!(
                "account rotation: {} walled {} run(s), no doctor report yet — probing next beat",
                batch.agent.id(),
                batch.runs.len()
            );
            continue;
        };
        if !lock(&state.inflight).claim(&batch) {
            continue;
        }
        for run in &batch.runs {
            lock(&state.holds).remove(&run.session_id);
        }
        spawn_probe(Arc::clone(state), batch, settings.clone(), report, data_dir.clone(), cx);
    }
}

/// Read one walled session's run record on the background executor and
/// cache it (`None` included: a run this host has no record of stays a
/// hold, not a read per beat).
fn read_record(state: Arc<RotationState>, session_id: String, data_dir: std::path::PathBuf, cx: &mut App) {
    if !lock(&state.reading).insert(session_id.clone()) {
        return;
    }
    cx.background_executor()
        .spawn(async move {
            let record = coding::run_registry::get(&data_dir, &session_id);
            lock(&state.records).insert(session_id.clone(), record);
            lock(&state.reading).remove(&session_id);
        })
        .detach();
}

/// ONE forced read of `batch.agent`'s profiles, however many of its runs
/// are walled, then every run decided off it and acted on in turn.
fn spawn_probe(
    state: Arc<RotationState>,
    batch: ProbeBatch,
    settings: coding::Settings,
    report: coding::DoctorReport,
    data_dir: std::path::PathBuf,
    cx: &mut App,
) {
    let sessions: Vec<&str> = batch.runs.iter().map(|run| run.session_id.as_str()).collect();
    log::info!(
        "account rotation [{}]: {} run(s) walled between turns — reading every {} profile's usage once",
        sessions.join(", "),
        batch.runs.len(),
        batch.agent.id()
    );
    cx.spawn(async move |cx| {
        let agent = batch.agent;
        let probe_dir = data_dir.clone();
        let profiles = cx
            .background_executor()
            .spawn(async move {
                let now_secs = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|elapsed| elapsed.as_secs())
                    .unwrap_or(0);
                coding::agent_usage::collect_now(agent, &probe_dir, &settings, &report, now_secs)
            })
            .await;
        let now_ms = chrono::Utc::now().timestamp_millis();
        let (decisions, chains) = {
            let mut tracker = lock(&state.tracker);
            let decisions = tracker.decide_batch(&batch.runs, &profiles, now_ms);
            (decisions, tracker.clone())
        };
        // The decisions counted: persisted before they act, off the
        // foreground, so a restart mid-switch still remembers them. Awaited,
        // so an undo's save below can never land before (and under) it.
        let save_dir = data_dir.clone();
        cx.background_executor()
            .spawn(async move { chains.save(&save_dir, now_ms) })
            .await;
        let mut undone = false;
        for (run, decision) in batch.runs.iter().zip(decisions) {
            let switched = cx.update(|cx| act(run, decision, cx));
            if !switched {
                // EXP-1107: a refused switch spends neither the cap nor the
                // cooldown; the chain is retried after the short spacing.
                lock(&state.tracker).undo_switch(&run.chain_key, now_ms, chrono::Utc::now().timestamp_millis());
                undone = true;
            }
        }
        if undone {
            let chains = lock(&state.tracker).clone();
            let now_ms = chrono::Utc::now().timestamp_millis();
            cx.background_executor()
                .spawn(async move { chains.save(&data_dir, now_ms) })
                .detach();
        }
        lock(&state.inflight).release(agent);
    })
    .detach();
}

/// Act on one decision. `false` = a switch the host could NOT make (the
/// caller undoes its count); a wait, or a switch under way, is `true`.
fn act(run: &WalledRun, decision: Decision, cx: &mut App) -> bool {
    let workflow = session_workflow(&run.session_id, cx);
    // The event sink is a blocking tRPC call: never on the foreground.
    let record = |kind: &str, message: String, cx: &App| {
        let Some((workflow_id, node_id)) = workflow.clone() else {
            return;
        };
        let Some(trpc) = crate::queries::trpc_client(cx) else {
            return;
        };
        let sink = TrpcEventSink::new(Arc::new(trpc));
        let event = api::workflows::WorkflowEvent {
            workflow_id,
            node_id,
            session_id: Some(run.session_id.clone()),
            kind: kind.to_string(),
            message,
        };
        cx.background_executor()
            .spawn(async move { sink.record(event) })
            .detach();
    };
    match decision {
        Decision::Switch {
            target,
            target_label,
            prompt,
            event_message,
        } => {
            log::info!(
                "account rotation [{}]: switching {} -> {target} ({target_label})",
                run.session_id,
                run.account
            );
            // The server inherits started_reason, the parent and the workflow
            // membership from the predecessor (EXP-1082 §1). A refusal (a
            // turn started meanwhile) hands back `false`: the caller undoes
            // the rotation the tracker counted (EXP-1107). A resume that
            // fails LATER (the old run did not stop in time) is reported by
            // `end_then_resume_on_account` itself and keeps its count — the
            // cooldown is then what spaces the retry.
            if crate::account_switch::end_then_resume_on_account(
                run.session_id.clone(),
                target,
                None,
                coding::LaunchOrigin::Local,
                Some(prompt),
                cx,
            ) {
                record("account_switched", event_message, cx);
                true
            } else {
                log::warn!(
                    "account rotation [{}]: switch refused — the agent started a turn; retrying soon",
                    run.session_id
                );
                false
            }
        }
        Decision::Wait {
            until_ms,
            event_message,
        } => {
            log::info!(
                "account rotation [{}]: no profile has headroom — waiting until {until_ms}: {event_message}",
                run.session_id
            );
            record("waiting_reset", event_message, cx);
            true
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_rate_limit_wall_becomes_a_walled_run() {
        let wall = steer::SessionBlocked {
            kind: steer::activity::BLOCKED_KIND_RATE_LIMIT.to_string(),
            agent: "claude".to_string(),
            window: "weekly".to_string(),
            resets_at: Some("2026-09-25T21:10:00.000Z".to_string()),
            since: "2026-09-25T18:00:00.000Z".to_string(),
        };
        let run = walled_run(
            "s1",
            std::path::Path::new("/tmp/wt"),
            coding::CodingAgent::Claude,
            &wall,
            None,
            true,
        )
        .expect("a rate-limit wall");
        assert_eq!(run.model, None);
        assert_eq!(run.chain_key, "/tmp/wt");
        assert_eq!(run.account, coding::SYSTEM_PROFILE);
        assert_eq!(run.window, "weekly");
        assert!(run.resets_at_ms.is_some());
        // No record: nothing to resume into, so the run is held like a
        // scratch one.
        assert!(run.repo_less);
        let other = steer::SessionBlocked {
            kind: "something_else".to_string(),
            ..wall
        };
        assert!(walled_run(
            "s1",
            std::path::Path::new("/tmp/wt"),
            coding::CodingAgent::Claude,
            &other,
            None,
            true
        )
        .is_none());
    }
}
