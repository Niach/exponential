//! EXP-481: the desktop's device-state sync loop — the headless daemon's
//! twin, per signed-in account.
//!
//! Since EXP-481 the SERVER is the source of truth for device state: the
//! devices row's `launch_defaults` is canonical (settings.json converges),
//! `device_worktrees` mirrors the local inventory, and owner→device work
//! (worktree remove/prune) queues in `device_commands`. This loop:
//!
//! * heartbeats every [`HEARTBEAT_INTERVAL`] — online-ness now derives from
//!   `last_seen_at` freshness (`contract::DEVICE_ONLINE_WINDOW_MS`), and the
//!   beat doubles as the WORK PULL (pending commands + the authoritative
//!   launch defaults on a stamp mismatch);
//! * reacts to the relay `check_in` nudge with an immediate beat, and
//!   (EXP-490) raises the same nudge itself when the synced `devices` shape
//!   moves the own row's launch-defaults stamp — a mobile/web edit converges
//!   within ~a tick instead of a full heartbeat interval;
//! * reports the worktree inventory (fingerprint-damped) on session
//!   start/end, after command execution, and on a slow cadence;
//! * converges launch defaults both ways — server edits apply to
//!   settings.json (through [`CodingHub`] so the pane + doctor + relay
//!   advertisement follow), local saves push up
//!   ([`push_local_defaults_if_changed`], called by `save_settings`).
//!
//! Threading: every git/tRPC touch runs on the background executor; entity
//! reads/writes marshal through `cx.update` (the steer_wiring pattern).

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use gpui::{App, AppContext as _, Global};

use crate::coding_flow::{CodingHub, LocalSessions};
use crate::queries;

/// EXP-481: 30s so one missed beat can't flap the 90s online window.
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(30);
/// The nudge-poll granularity inside the beat loop.
const TICK: Duration = Duration::from_secs(1);
/// Inventory rescans are git work (one status per worktree) — off-cadence
/// scans only fire on real triggers; this is the slow safety cadence.
const INVENTORY_BEATS: u32 = 10;

#[derive(Default)]
struct DeviceSyncState {
    /// Stop flag per account (sign-out flips it; the loop retires itself).
    by_account: HashMap<String, Arc<AtomicBool>>,
    /// EXP-490: the devices-shape watch per account (dropped on sign-out).
    watch_by_account: HashMap<String, gpui::Subscription>,
    /// The relay `check_in` flag — a nudge beats immediately.
    check_in: Arc<AtomicBool>,
    /// An out-of-cadence inventory report request (session start/end,
    /// command completion, a local prune).
    report_soon: Arc<AtomicBool>,
    /// One command batch / prune at a time.
    worker_busy: Arc<AtomicBool>,
    /// EXP-484 (D): the `agent_login` command ids this process is already
    /// running. A pending command rides EVERY heartbeat response until it is
    /// completed, and a login is completed only once its URL is on the grid —
    /// without this claim set the next beat would open a second tab.
    inflight_logins: Arc<Mutex<HashSet<String>>>,
}

struct DeviceSyncGlobal(gpui::Entity<DeviceSyncState>);
impl Global for DeviceSyncGlobal {}

fn state(cx: &mut App) -> gpui::Entity<DeviceSyncState> {
    if let Some(global) = cx.try_global::<DeviceSyncGlobal>() {
        return global.0.clone();
    }
    let entity = cx.new(|_| DeviceSyncState::default());
    cx.set_global(DeviceSyncGlobal(entity.clone()));
    entity
}

/// The relay `check_in` hook ([`crate::steer_wiring`] hands it to the
/// control channel) — non-blocking by contract.
pub fn check_in_flag(cx: &mut App) -> Arc<AtomicBool> {
    let state = state(cx);
    state.read(cx).check_in.clone()
}

/// Ask for an out-of-cadence inventory report (a local prune just ran, a
/// session registered/ended).
pub fn report_soon(cx: &mut App) {
    let state = state(cx);
    state.read(cx).report_soon.store(true, Ordering::SeqCst);
}

/// EXP-484: beat NOW — used by [`CodingHub::refresh_agent_usage`] after a
/// login so the re-probed accounts (and any usage the poll policy lets
/// through) reach the row without waiting out the interval.
pub(crate) fn beat_soon(cx: &mut App) {
    let state = state(cx);
    state.read(cx).check_in.store(true, Ordering::SeqCst);
}

/// EXP-484 (D): the in-flight `agent_login` claim set.
pub(crate) fn inflight_logins(cx: &mut App) -> Arc<Mutex<HashSet<String>>> {
    let state = state(cx);
    state.read(cx).inflight_logins.clone()
}

/// Release a finished login's claim (its tab exited, or it never started).
/// A redelivery after this legitimately re-runs it — the command is only
/// still pending if `completeCommand` never landed.
pub(crate) fn release_login(command_id: &str, cx: &mut App) {
    if let Ok(mut inflight) = inflight_logins(cx).lock() {
        inflight.remove(command_id);
    }
}

/// Claim `id` for execution; `false` = this process is already running it
/// (a heartbeat redelivery), so the caller does NOTHING — neither a second
/// tab nor a completion.
fn claim_login(inflight: &Mutex<HashSet<String>>, id: &str) -> bool {
    match inflight.lock() {
        Ok(mut inflight) => inflight.insert(id.to_string()),
        // A poisoned lock must not spawn tabs in a loop — refuse.
        Err(_) => false,
    }
}

/// Start the loop for `account` (from `session::connect_account`, beside
/// `start_control_channel`). Restarting for the same account replaces the
/// old loop.
pub fn start_device_sync(account: &api::Account, cx: &mut App) {
    let state_entity = state(cx);
    let account_id = account.id.clone();
    let stop = Arc::new(AtomicBool::new(false));
    let (check_in, report_soon, worker_busy) = state_entity.update(cx, |state, _| {
        if let Some(previous) = state.by_account.insert(account_id.clone(), stop.clone()) {
            previous.store(true, Ordering::SeqCst);
        }
        (
            state.check_in.clone(),
            state.report_soon.clone(),
            state.worker_busy.clone(),
        )
    });
    // First-connect state: report the inventory once sessions settle.
    report_soon.store(true, Ordering::SeqCst);

    // EXP-490: realtime convergence — watch the synced devices shape and
    // raise the check_in nudge when the own row's launch-defaults stamp
    // moves, so a mobile/web edit beats on the next tick.
    let watch = watch_devices_shape(stop.clone(), check_in.clone(), cx);
    state_entity.update(cx, |state, _| match watch {
        Some(watch) => {
            state.watch_by_account.insert(account_id.clone(), watch);
        }
        None => {
            state.watch_by_account.remove(&account_id);
        }
    });

    cx.spawn(async move |cx| {
        let mut ticks_since_beat = u32::MAX / 2; // beat immediately
        let mut beats_since_inventory = INVENTORY_BEATS; // scan on the first beat
        let mut last_inventory_fp: Option<u64> = None;
        // EXP-484: what this loop last SENT, so an unchanged agent status
        // never re-stamps `agent_usage_at` (a write every beat would make
        // the row churn for nothing).
        let sent_status = Arc::new(Mutex::new(AgentStatusSent::default()));
        loop {
            cx.background_executor().timer(TICK).await;
            if stop.load(Ordering::SeqCst) {
                return;
            }
            ticks_since_beat = ticks_since_beat.saturating_add(1);
            let nudged = check_in.swap(false, Ordering::SeqCst);
            if !nudged && Duration::from_secs(ticks_since_beat as u64) < HEARTBEAT_INTERVAL {
                continue;
            }
            ticks_since_beat = 0;

            // Foreground snapshot: live sessions + the account's client.
            let Some(snapshot) = cx.update(|cx| snapshot_for(&account_id, cx)) else {
                // Account switched away — retire; connect_account restarts.
                return;
            };
            beats_since_inventory = beats_since_inventory.saturating_add(1);
            let scan_due = snapshot.report_requested
                || report_soon.swap(false, Ordering::SeqCst)
                || beats_since_inventory >= INVENTORY_BEATS;

            let worker_busy = worker_busy.clone();
            let sent_status = sent_status.clone();
            let previous_fp = last_inventory_fp;
            let outcome = cx
                .background_executor()
                .spawn({
                    let snapshot = snapshot.clone();
                    async move {
                        beat(&snapshot, scan_due, previous_fp, &worker_busy, &sent_status)
                    }
                })
                .await;
            last_inventory_fp = outcome.inventory_fp;
            if scan_due {
                beats_since_inventory = 0;
            }
            if let Some(status) = outcome.agent_status {
                // EXP-484: the toolbar and the device dialog read the OWN
                // machine's numbers from here, not from the round trip.
                let _ = cx.update(|cx| {
                    let hub = CodingHub::global(cx);
                    hub.update(cx, |hub, cx| {
                        // Always adopt the fresh stamps, but only repaint
                        // when the IDENTITY or the numbers moved: every pass
                        // restamps `checkedAt`, and a notify per beat would
                        // re-render the toolbar and the dialog for nothing.
                        let changed = agent_status_changed(hub.agent_status.as_ref(), &status);
                        hub.agent_status = Some(status);
                        if changed {
                            cx.notify();
                        }
                    });
                });
            }
            // EXP-484 (D): an `agent_login` command runs on the FOREGROUND —
            // it opens a terminal tab and watches its grid.
            for command in outcome.deferred {
                let _ = cx.update(|cx| crate::agent_login::start_remote_login(command, cx));
            }
            if outcome.defaults_changed {
                // The server copy landed in settings.json — reload the hub
                // so the Agents pane, the doctor and the relay advertisement
                // all follow (refresh_doctor → restart_control_channel).
                let _ = cx.update(|cx| reload_hub_settings(cx));
            }
        }
    })
    .detach();
}

/// Stop `account_id`'s loop (from the sign-out paths, beside
/// `stop_control_channel`).
pub fn stop_device_sync(account_id: &str, cx: &mut App) {
    let state = state(cx);
    state.update(cx, |state, _| {
        if let Some(stop) = state.by_account.remove(account_id) {
            stop.store(true, Ordering::SeqCst);
        }
        // Dropping the subscription detaches the shape watch; the stop flag
        // covers a same-tick notification already in flight.
        state.watch_by_account.remove(account_id);
    });
}

/// EXP-490: the shape-watch latch. Records `observed`; nudges only when a
/// PREVIOUSLY observed stamp changed. The first observation never nudges
/// (the loop's immediate first beat covers startup), and unchanged deltas —
/// `last_seen_at`/`active_sessions` noise from every device's beats,
/// including the Electric echo of our own — MUST stay silent, or the echo
/// would re-nudge the loop forever.
fn stamp_moved(last: &mut Option<Option<String>>, observed: Option<String>) -> bool {
    match last {
        Some(previous) if *previous == observed => false,
        Some(_) => {
            *last = Some(observed);
            true
        }
        None => {
            *last = Some(observed);
            false
        }
    }
}

/// Observe the synced `devices` collection and flip `check_in` when the OWN
/// row's `launch_defaults_updated_at` moves. Keys on the GUI's
/// [`steer::persistent_device_id`] — never the CLI daemon's sibling row.
/// `None` when the sync store is absent (headless tests); the heartbeat
/// cadence remains the fallback either way.
///
/// EXP-484: the watch stays keyed on `launch_defaults_updated_at` ALONE.
/// `agent_usage_at` moves every 3–10 minutes on its own (the collector's
/// poll policy), so nudging on it would turn every usage refresh into an
/// extra beat — which writes the row again, which nudges again.
fn watch_devices_shape(
    stop: Arc<AtomicBool>,
    check_in: Arc<AtomicBool>,
    cx: &mut App,
) -> Option<gpui::Subscription> {
    let devices = sync::Store::try_global(cx)?.collections().devices.clone();
    let device_id =
        steer::persistent_device_id(&crate::session::AuthContext::global(cx).data_dir);
    let mut last_stamp: Option<Option<String>> = None;
    Some(cx.observe(&devices, move |devices, cx| {
        if stop.load(Ordering::SeqCst) {
            return;
        }
        let Some(observed) = devices
            .read(cx)
            .iter()
            .find(|row| row.device_id.as_deref() == Some(device_id.as_str()))
            .map(|row| row.launch_defaults_updated_at.clone())
        else {
            return; // own row not synced (yet) — nothing to converge from
        };
        if stamp_moved(&mut last_stamp, observed) {
            check_in.store(true, Ordering::SeqCst);
        }
    }))
}

// ---------------------------------------------------------------------------
// The blocking beat body (background executor)
// ---------------------------------------------------------------------------

/// Everything the background beat needs, snapshotted on the foreground.
#[derive(Clone)]
struct BeatSnapshot {
    trpc: Arc<api::TrpcClient>,
    device_id: String,
    /// The app data dir — EXP-637's `runs.json` lives here, and the remote
    /// prune command nominates its recorded run branches.
    data_dir: PathBuf,
    settings_path: PathBuf,
    repos_root: PathBuf,
    branch_prefix: String,
    held_branches: HashSet<String>,
    active_sessions: u32,
    report_requested: bool,
    /// EXP-484: the live settings + doctor report the agent-status collector
    /// runs against. `None` doctor = the first probe has not landed yet;
    /// the beat then simply reports no agent status.
    settings: coding::Settings,
    doctor: Option<coding::DoctorReport>,
    /// EXP-484 (D): the shared `agent_login` claim set (see [`claim_login`]).
    inflight_logins: Arc<Mutex<HashSet<String>>>,
}

/// What this loop last SENT, so an unchanged payload is left off the beat
/// entirely (the server stamps `agent_usage_at` on every write, and that
/// column moves the row for every synced client).
///
/// The two halves compare DIFFERENTLY on purpose. Every collection pass
/// restamps `checkedAt` on every account, so comparing serialized accounts
/// JSON would ship a write on all 30s — the identity key
/// (`agent_accounts::accounts_key`, sans `checked_at`) is the real change
/// signal. Usage compares as JSON: its `fetched_at` only moves on an actual
/// fetch, which IS a change worth sending.
#[derive(Default)]
struct AgentStatusSent {
    accounts_key: Option<String>,
    usage: Option<String>,
}

/// What one beat has to write, with the accounts identity key that decides
/// it (recorded only once the server accepts the beat).
#[derive(Default)]
struct StatusWrites {
    accounts: Option<serde_json::Value>,
    accounts_key: Option<String>,
    usage: Option<serde_json::Value>,
}

fn snapshot_for(account_id: &str, cx: &mut App) -> Option<BeatSnapshot> {
    let account = queries::active_account(cx)?;
    if account.id != account_id {
        return None;
    }
    let trpc = Arc::new(queries::trpc_client(cx)?);
    let data_dir = crate::session::AuthContext::global(cx).data_dir.clone();
    let device_id = steer::persistent_device_id(&data_dir);
    let settings_path = coding::Settings::default_path(&data_dir);
    let sessions = LocalSessions::global(cx);
    let sessions = sessions.read(cx);
    let held_branches: HashSet<String> =
        sessions.held_branches().map(str::to_string).collect();
    let active_sessions = sessions.session_ids().len() as u32;
    let hub = CodingHub::global(cx);
    let (settings, doctor) = {
        let hub = hub.read(cx);
        (hub.settings.clone(), hub.doctor.report.clone())
    };
    let inflight_logins = inflight_logins(cx);
    Some(BeatSnapshot {
        trpc,
        device_id,
        data_dir,
        repos_root: settings.repos_root_path(),
        branch_prefix: settings.branch_prefix.clone(),
        settings_path,
        held_branches,
        active_sessions,
        report_requested: false,
        settings,
        doctor,
        inflight_logins,
    })
}

struct BeatOutcome {
    inventory_fp: Option<u64>,
    defaults_changed: bool,
    /// EXP-484: this pass's agent status (accounts + usage), for the hub
    /// snapshot the toolbar and the device dialog read.
    agent_status: Option<coding::agent_usage::AgentStatusPayload>,
    /// EXP-484 (D): commands that must run on the FOREGROUND (an
    /// `agent_login` opens a terminal tab), already claimed.
    deferred: Vec<api::devices::PendingCommand>,
}

/// What [`run_device_command`] did with one command.
enum CommandDisposition {
    /// Ran (or refused) it and reported back.
    Completed,
    /// Claimed it for the foreground — nothing has been reported yet.
    Deferred(api::devices::PendingCommand),
    /// A redelivery of a command this process is already running: do
    /// NOTHING. Completing it would answer the in-flight run's own row.
    AlreadyRunning,
}

/// One heartbeat + work pull + (when due) inventory report. Blocking.
fn beat(
    snapshot: &BeatSnapshot,
    scan_due: bool,
    last_fp: Option<u64>,
    worker_busy: &Arc<AtomicBool>,
    sent_status: &Mutex<AgentStatusSent>,
) -> BeatOutcome {
    let mut last_fp = last_fp;
    let mut defaults_changed = false;
    let mut deferred = Vec::new();
    let synced_at = coding::read_marker(&snapshot.settings_path, &snapshot.device_id).synced_at;

    // EXP-484: refresh whatever the poll policy says is due (keychain reads,
    // one HTTPS GET, a `codex app-server` spawn — seconds of blocking work,
    // which is why this only ever runs here, on the background executor) and
    // attach the result only when it CHANGED. `agent_usage_at` is stamped on
    // every accepted write, and that column is synced to every client.
    let agent_status = snapshot.doctor.as_ref().map(|report| {
        coding::agent_usage::collect_if_due(
            &snapshot.data_dir,
            &snapshot.settings,
            report,
            now_unix_secs(),
        )
    });
    let writes = pending_status_writes(agent_status.as_ref(), sent_status);

    match api::devices::heartbeat(
        &snapshot.trpc,
        &api::devices::HeartbeatInput {
            device_id: &snapshot.device_id,
            active_sessions: snapshot.active_sessions,
            defaults_synced_at: synced_at.as_deref(),
            // EXP-484 (A3): both are `None` on a beat with nothing new to
            // say, which keeps the historic body byte-for-byte.
            agent_accounts: writes.accounts.as_ref(),
            agent_usage: writes.usage.as_ref(),
        },
    ) {
        Ok(result) => {
            // Accepted — remember what the row now carries, so the next
            // unchanged pass sends nothing.
            record_status_sent(&writes, sent_status);
            // `ok: false` (row removed) heals via the control channel's own
            // register on its next re-dial; the beat keeps running.
            if result.launch_defaults.is_some() || result.launch_defaults_updated_at.is_some() {
                defaults_changed = reconcile_server_defaults(
                    snapshot,
                    result.launch_defaults.as_ref(),
                    result.launch_defaults_updated_at.as_deref(),
                );
            } else if result.ok {
                // No payload = the server judged the stamps EQUAL. Queued
                // dirty pushes, hand-edit drift and the first-ever seed must
                // still act (EXP-490 — the retry the module doc promises).
                defaults_changed = reconcile_local_defaults(snapshot);
            }
            if !result.commands.is_empty() {
                // One batch at a time — a long prune must not pile up.
                if worker_busy
                    .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
                    .is_ok()
                {
                    for command in &result.commands {
                        match run_device_command(snapshot, command) {
                            CommandDisposition::Deferred(command) => deferred.push(command),
                            CommandDisposition::Completed
                            | CommandDisposition::AlreadyRunning => {}
                        }
                    }
                    worker_busy.store(false, Ordering::SeqCst);
                    // Commands change the inventory — rescan below.
                    last_fp = None;
                }
            }
        }
        Err(err) => log::debug!("[device-sync] heartbeat failed: {err}"),
    }

    let inventory_fp = if scan_due || last_fp.is_none() {
        report_worktrees(snapshot, last_fp)
    } else {
        last_fp
    };
    BeatOutcome {
        inventory_fp,
        defaults_changed,
        agent_status,
        deferred,
    }
}

fn now_unix_secs() -> u64 {
    chrono::Utc::now().timestamp().max(0) as u64
}

/// Which halves of the agent status this beat has to write: the serialized
/// value when it differs from the last accepted one, `None` when it doesn't
/// (see [`AgentStatusSent`] for why the two halves compare differently).
fn pending_status_writes(
    status: Option<&coding::agent_usage::AgentStatusPayload>,
    sent: &Mutex<AgentStatusSent>,
) -> StatusWrites {
    let Some(status) = status else {
        return StatusWrites::default();
    };
    let Ok(sent) = sent.lock() else {
        return StatusWrites::default();
    };
    let key = coding::agent_accounts::accounts_key(&status.accounts);
    let accounts = status
        .accounts_json()
        .filter(|_| sent.accounts_key.as_deref() != Some(key.as_str()));
    let usage = status
        .usage_json()
        .filter(|usage| Some(usage.to_string()) != sent.usage);
    StatusWrites {
        accounts_key: accounts.is_some().then_some(key),
        accounts,
        usage,
    }
}

/// Record what the server accepted (only after an `Ok` heartbeat — a failed
/// beat must resend on the next one).
fn record_status_sent(writes: &StatusWrites, sent: &Mutex<AgentStatusSent>) {
    let Ok(mut sent) = sent.lock() else {
        return;
    };
    if let Some(key) = writes.accounts_key.clone() {
        sent.accounts_key = Some(key);
    }
    if let Some(usage) = &writes.usage {
        sent.usage = Some(usage.to_string());
    }
}

/// Whether a freshly collected status is a real change from the one the hub
/// already holds — the same identity rule the heartbeat uses, so a pass that
/// only restamped `checkedAt` does not repaint every surface reading it.
fn agent_status_changed(
    previous: Option<&coding::agent_usage::AgentStatusPayload>,
    next: &coding::agent_usage::AgentStatusPayload,
) -> bool {
    let Some(previous) = previous else {
        return true;
    };
    coding::agent_accounts::accounts_key(&previous.accounts)
        != coding::agent_accounts::accounts_key(&next.accounts)
        || previous.usage != next.usage
}

/// Apply/push against an OBSERVED server defaults copy. Returns whether
/// settings.json changed (the caller reloads the hub).
fn reconcile_server_defaults(
    snapshot: &BeatSnapshot,
    server_defaults: Option<&serde_json::Value>,
    server_stamp: Option<&str>,
) -> bool {
    let settings = coding::Settings::load(&snapshot.settings_path);
    let fingerprint = coding::defaults_fingerprint(&settings);
    let marker = coding::read_marker(&snapshot.settings_path, &snapshot.device_id);
    match coding::reconcile(
        &marker,
        &fingerprint,
        server_stamp,
        server_defaults.is_some(),
    ) {
        coding::ReconcileAction::Noop => false,
        coding::ReconcileAction::ApplyServer => match server_defaults {
            Some(value) => apply_server_defaults(snapshot, value, server_stamp),
            None => push_defaults(snapshot, &settings, &marker),
        },
        coding::ReconcileAction::PushLocal => push_defaults(snapshot, &settings, &marker),
    }
}

/// No fresh server copy this beat — the stamps matched, so reconcile purely
/// against local state (dirty retry, hand-edit drift, first-ever seed).
/// `ApplyServer` is unreachable: the observed stamp IS the marker's.
fn reconcile_local_defaults(snapshot: &BeatSnapshot) -> bool {
    let settings = coding::Settings::load(&snapshot.settings_path);
    let fingerprint = coding::defaults_fingerprint(&settings);
    let marker = coding::read_marker(&snapshot.settings_path, &snapshot.device_id);
    match coding::reconcile(
        &marker,
        &fingerprint,
        marker.synced_at.as_deref(),
        marker.synced_at.is_some(),
    ) {
        coding::ReconcileAction::PushLocal => push_defaults(snapshot, &settings, &marker),
        _ => false,
    }
}

fn apply_server_defaults(
    snapshot: &BeatSnapshot,
    value: &serde_json::Value,
    stamp: Option<&str>,
) -> bool {
    let patch: coding::DefaultsPatch = match serde_json::from_value(value.clone()) {
        Ok(patch) => patch,
        Err(err) => {
            log::warn!("[device-sync] unparsable server defaults ignored: {err}");
            return false;
        }
    };
    let mut settings = coding::Settings::load(&snapshot.settings_path);
    let changed = coding::apply_defaults_patch(&mut settings, &patch);
    if changed {
        if let Err(err) = settings.save(&snapshot.settings_path) {
            log::warn!("[device-sync] defaults save failed: {err}");
            return false;
        }
        log::info!("[device-sync] applied server launch defaults");
    }
    // Clamped fields are NOT pushed back (ping-pong); the stamp still
    // records so the apply never loops.
    let marker = coding::SyncMarker {
        synced_at: stamp.map(str::to_string),
        dirty: false,
        hash: Some(coding::defaults_fingerprint(&settings)),
    };
    let _ = coding::write_marker(&snapshot.settings_path, &snapshot.device_id, &marker);
    changed
}

/// Returns whether settings.json changed (only the conflict arm can — an
/// adopted server copy must reload the hub like any other apply).
fn push_defaults(
    snapshot: &BeatSnapshot,
    settings: &coding::Settings,
    marker: &coding::SyncMarker,
) -> bool {
    let wire = serde_json::to_value(coding::defaults_wire(settings))
        .expect("defaults serialize cannot fail");
    let expected = api::devices::ExpectedStamp::Expect(marker.synced_at.as_deref());
    match api::devices::set_launch_defaults(&snapshot.trpc, &snapshot.device_id, &wire, expected) {
        Ok(result) if result.conflict => {
            log::info!("[device-sync] defaults push conflicted — adopting the server copy");
            match result.launch_defaults.as_ref() {
                Some(value) => apply_server_defaults(
                    snapshot,
                    value,
                    result.launch_defaults_updated_at.as_deref(),
                ),
                None => false,
            }
        }
        Ok(result) => {
            let marker = coding::SyncMarker {
                synced_at: result.launch_defaults_updated_at,
                dirty: false,
                hash: Some(coding::defaults_fingerprint(settings)),
            };
            let _ = coding::write_marker(&snapshot.settings_path, &snapshot.device_id, &marker);
            false
        }
        Err(err) => {
            log::debug!("[device-sync] defaults push failed ({err}) — queued");
            let queued = coding::SyncMarker {
                dirty: true,
                synced_at: marker.synced_at.clone(),
                hash: marker.hash.clone(),
            };
            let _ = coding::write_marker(&snapshot.settings_path, &snapshot.device_id, &queued);
            false
        }
    }
}

/// EXP-481: called by `CodingHub::save_settings` AFTER a successful save —
/// push the launch defaults up when they changed (offline queues via the
/// dirty marker; the beat retries). Background-executed by the caller.
pub(crate) fn push_local_defaults_if_changed(
    trpc: api::TrpcClient,
    settings_path: PathBuf,
    data_dir: PathBuf,
    device_id: String,
    settings: coding::Settings,
) {
    let marker = coding::read_marker(&settings_path, &device_id);
    let fingerprint = coding::defaults_fingerprint(&settings);
    if !marker.dirty && marker.hash.as_deref() == Some(fingerprint.as_str()) {
        return; // paths/prefs-only save — nothing launch-default changed
    }
    let snapshot = BeatSnapshot {
        trpc: Arc::new(trpc),
        device_id,
        data_dir: data_dir.clone(),
        settings_path,
        repos_root: settings.repos_root_path(),
        branch_prefix: settings.branch_prefix.clone(),
        held_branches: HashSet::new(),
        active_sessions: 0,
        report_requested: false,
        // Defaults-push only — this snapshot never beats, collects or runs
        // a command.
        settings: settings.clone(),
        doctor: None,
        inflight_logins: Arc::new(Mutex::new(HashSet::new())),
    };
    push_defaults(&snapshot, &settings, &marker);
}

// ---------------------------------------------------------------------------
// Commands + inventory (the daemon's run_device_command twin)
// ---------------------------------------------------------------------------

fn run_device_command(
    snapshot: &BeatSnapshot,
    command: &api::devices::PendingCommand,
) -> CommandDisposition {
    // EXP-484 (D): a login is not a background job — it opens a terminal
    // tab and is answered the moment its URL is on the grid, so it is
    // CLAIMED here and handed to the foreground. A redelivery of a claimed
    // id is dropped without a completion.
    if command.kind == "agent_login" {
        let agent = command.payload["agent"].as_str().unwrap_or_default();
        let switch = command.payload["switch"].as_str().unwrap_or("false");
        let refusal = match (coding::CodingAgent::parse(agent), switch) {
            // pi's sign-in is an interactive prompt with no device-code flow
            // to hand back — local only (the server refuses it too).
            (Some(coding::CodingAgent::Pi), _) => Some("pi has no remote sign-in"),
            (None, _) => Some("This machine does not know that agent."),
            (Some(_), "true") | (Some(_), "false") => None,
            (Some(_), _) => Some("Malformed command payload."),
        };
        if let Some(refusal) = refusal {
            complete(snapshot, &command.id, false, refusal);
            return CommandDisposition::Completed;
        }
        if !claim_login(&snapshot.inflight_logins, &command.id) {
            return CommandDisposition::AlreadyRunning;
        }
        return CommandDisposition::Deferred(command.clone());
    }
    let (ok, message) = match command.kind.as_str() {
        "worktree_remove" => {
            let repo = command.payload["repoFullName"].as_str().unwrap_or_default();
            let branch = command.payload["branch"].as_str().unwrap_or_default();
            if repo.is_empty() || branch.is_empty() {
                (false, "Malformed command payload.".to_string())
            } else {
                let clone = coding::clone_path(&snapshot.repos_root, repo);
                match coding::remove_worktree_remote(&clone, branch, &snapshot.held_branches) {
                    Ok(()) => (true, format!("Removed the {branch} worktree.")),
                    Err(err) => (false, err.message()),
                }
            }
        }
        "worktree_prune" => {
            let policy = coding::conservative_prune_policy(
                &snapshot.branch_prefix,
                snapshot.held_branches.clone(),
                Vec::new(),
                // EXP-637: nominate this install's own recorded run branches
                // too (git still has to confirm they landed).
                Some(snapshot.data_dir.clone()),
            );
            let mut removed = 0usize;
            let mut skipped = 0usize;
            let mut blocked = false;
            for clone in coding::scan_clones(&snapshot.repos_root) {
                let report = coding::prune_landed(&clone.path, &policy);
                removed += report.removed_worktrees.len();
                skipped += report.skipped.len();
                blocked |= report.blocked_by_launch;
            }
            let mut message =
                format!("Pruned {removed} worktree{}", if removed == 1 { "" } else { "s" });
            if skipped > 0 {
                message.push_str(&format!(", kept {skipped} (unmerged or busy)"));
            }
            if blocked {
                message.push_str("; one repo was busy launching — try again");
            }
            message.push('.');
            (true, message)
        }
        other => {
            log::info!("[device-sync] command {other:?} unsupported — reported back");
            (
                false,
                "This machine's app doesn't support that command yet.".to_string(),
            )
        }
    };
    complete(snapshot, &command.id, ok, &message);
    CommandDisposition::Completed
}

fn complete(snapshot: &BeatSnapshot, command_id: &str, ok: bool, message: &str) {
    if let Err(err) = api::devices::complete_command(&snapshot.trpc, command_id, ok, Some(message))
    {
        log::debug!("[device-sync] completeCommand failed (redelivery retries): {err}");
    }
}

/// Scan + report when the fingerprint moved; returns the fingerprint to
/// remember (`None` = report failed, retry next time).
fn report_worktrees(snapshot: &BeatSnapshot, last_fp: Option<u64>) -> Option<u64> {
    let inventory = coding::scan_inventory(&snapshot.repos_root);
    let fp = coding::inventory_fingerprint(&inventory, &snapshot.held_branches);
    if last_fp == Some(fp) {
        return last_fp;
    }
    let agent_ids: Vec<Vec<String>> = inventory
        .iter()
        .map(|entry| {
            entry
                .agents
                .as_ref()
                .map(|agents| agents.iter().map(|agent| agent.id().to_string()).collect())
                .unwrap_or_default()
        })
        .collect();
    let rows: Vec<api::devices::WorktreeReportEntry> = inventory
        .iter()
        .zip(agent_ids.iter())
        .map(|(entry, ids)| api::devices::WorktreeReportEntry {
            repo_full_name: &entry.repo,
            branch: &entry.branch,
            issue_identifier: entry.issue_identifier(),
            agents: entry.agents.as_ref().map(|_| ids.as_slice()),
            dirty: entry.dirty_wire(),
            busy: snapshot.held_branches.contains(&entry.branch),
        })
        .collect();
    match api::devices::report_worktrees(&snapshot.trpc, &snapshot.device_id, &rows) {
        Ok(()) => Some(fp),
        Err(err) => {
            log::debug!("[device-sync] reportWorktrees failed: {err}");
            last_fp
        }
    }
}

fn reload_hub_settings(cx: &mut App) {
    let auth = crate::session::AuthContext::global(cx);
    let settings_path = coding::Settings::default_path(&auth.data_dir);
    let hub = CodingHub::global(cx);
    hub.update(cx, |hub, cx| {
        hub.settings = coding::Settings::load(&settings_path);
        cx.notify();
    });
    // Doctor refresh re-derives the advertisement → the relay re-dials with
    // the new defaults (the EXP-437 path).
    CodingHub::refresh_doctor(&hub, cx);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// EXP-490: the anti-feedback-loop lock — heartbeats mutate the devices
    /// row (`last_seen_at`/`active_sessions`) every ~30s per device, and the
    /// Electric echo of our own beat must never re-nudge the loop.
    ///
    /// EXP-484 note: the watch this latch guards observes ONLY
    /// `launch_defaults_updated_at`. `agent_usage_at` moves on the
    /// collector's own 3–10 minute cadence and is deliberately not a nudge
    /// trigger — a latch this careful would still see one beat per refresh.
    #[test]
    fn stamp_latch_truth_table() {
        let stamp = |s: &str| Some(s.to_string());

        // First observation records without nudging (the loop's immediate
        // first beat covers startup).
        let mut last = None;
        assert!(!stamp_moved(&mut last, stamp("a")));
        // Unchanged stamp (last_seen_at noise) never nudges.
        assert!(!stamp_moved(&mut last, stamp("a")));
        // A moved stamp nudges once, then latches.
        assert!(stamp_moved(&mut last, stamp("b")));
        assert!(!stamp_moved(&mut last, stamp("b")));
        // Some -> None (defaults cleared server-side) is a move.
        assert!(stamp_moved(&mut last, None));
        assert!(!stamp_moved(&mut last, None));
        // None -> Some after the first observation is a move too.
        assert!(stamp_moved(&mut last, stamp("c")));

        // First observation of a None stamp also just records.
        let mut last = None;
        assert!(!stamp_moved(&mut last, None));
        assert!(stamp_moved(&mut last, stamp("a")));
    }

    /// EXP-484 (D): a pending `agent_login` rides EVERY heartbeat response
    /// until it is completed, and it is only completed once its URL is on
    /// the grid — so the second delivery must find the id claimed and do
    /// nothing at all (a second tab, or a completion of the live run's own
    /// row, are both wrong).
    #[test]
    fn inflight_login_dedupes_redelivered_ids() {
        let inflight = Mutex::new(HashSet::new());
        assert!(claim_login(&inflight, "cmd-1"), "first delivery runs");
        assert!(!claim_login(&inflight, "cmd-1"), "redelivery is a no-op");
        // A different command is unaffected.
        assert!(claim_login(&inflight, "cmd-2"));
        // Released (the tab exited) — a later redelivery may run again,
        // which only happens when completeCommand never landed.
        inflight.lock().unwrap().remove("cmd-1");
        assert!(claim_login(&inflight, "cmd-1"));
    }

    /// The agent status rides a beat only when it CHANGED — `agent_usage_at`
    /// is stamped on every accepted write, and that column syncs to every
    /// client. Crucially, EVERY collection pass restamps `checkedAt`, so the
    /// accounts half compares on IDENTITY, not on serialized JSON.
    #[test]
    fn agent_status_is_attached_only_when_it_changed() {
        use coding::agent_usage::{AgentStatusPayload, AgentUsage};

        let mut status = AgentStatusPayload::default();
        status.accounts.insert(
            "claude".to_string(),
            coding::agent_accounts::AgentAccount {
                signed_in: true,
                email: Some("dev@acme.test".to_string()),
                plan: Some("max".to_string()),
                checked_at: "2026-08-28T10:00:00.000Z".to_string(),
            },
        );
        status.usage.insert(
            "claude".to_string(),
            AgentUsage {
                fetched_at: "2026-08-28T10:00:00.000Z".to_string(),
                stale: false,
                windows: Vec::new(),
            },
        );

        let sent = Mutex::new(AgentStatusSent::default());
        let writes = pending_status_writes(Some(&status), &sent);
        assert!(
            writes.accounts.is_some() && writes.usage.is_some(),
            "first beat writes both"
        );
        // A failed beat records nothing — the next one resends.
        let again = pending_status_writes(Some(&status), &sent);
        assert!(again.accounts.is_some() && again.usage.is_some());
        record_status_sent(&writes, &sent);
        let writes = pending_status_writes(Some(&status), &sent);
        assert!(
            writes.accounts.is_none() && writes.usage.is_none(),
            "unchanged sends nothing"
        );

        // The every-pass restamp is NOT a change: same account, new
        // `checkedAt` → still nothing to write (otherwise the row would take
        // a write, and every client a delta, every 30 seconds).
        let restamped = "2026-08-28T10:00:30.000Z".to_string();
        status.accounts.get_mut("claude").unwrap().checked_at = restamped.clone();
        let writes = pending_status_writes(Some(&status), &sent);
        assert!(writes.accounts.is_none(), "a restamped checkedAt is not a change");
        assert!(!agent_status_changed(Some(&status.clone()), &status));

        // A moved number writes again — and only the half that moved.
        status.usage.get_mut("claude").unwrap().stale = true;
        let writes = pending_status_writes(Some(&status), &sent);
        assert!(writes.accounts.is_none());
        assert!(writes.usage.is_some());
        record_status_sent(&writes, &sent);

        // A different ACCOUNT (the switch this feature exists for) does.
        let mut switched = status.clone();
        switched.accounts.get_mut("claude").unwrap().email =
            Some("other@acme.test".to_string());
        let writes = pending_status_writes(Some(&switched), &sent);
        assert!(writes.accounts.is_some(), "a new identity always writes");
        assert!(agent_status_changed(Some(&status), &switched));

        // No doctor report yet = nothing to say.
        let writes = pending_status_writes(None, &sent);
        assert!(writes.accounts.is_none() && writes.usage.is_none());
    }
}
