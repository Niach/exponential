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
//! * reacts to the relay `check_in` nudge with an immediate beat;
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
use std::sync::Arc;
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
    /// The relay `check_in` flag — a nudge beats immediately.
    check_in: Arc<AtomicBool>,
    /// An out-of-cadence inventory report request (session start/end,
    /// command completion, a local prune).
    report_soon: Arc<AtomicBool>,
    /// One command batch / prune at a time.
    worker_busy: Arc<AtomicBool>,
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

    cx.spawn(async move |cx| {
        let mut ticks_since_beat = u32::MAX / 2; // beat immediately
        let mut beats_since_inventory = INVENTORY_BEATS; // scan on the first beat
        let mut last_inventory_fp: Option<u64> = None;
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
            let previous_fp = last_inventory_fp;
            let outcome = cx
                .background_executor()
                .spawn({
                    let snapshot = snapshot.clone();
                    async move { beat(&snapshot, scan_due, previous_fp, &worker_busy) }
                })
                .await;
            last_inventory_fp = outcome.inventory_fp;
            if scan_due {
                beats_since_inventory = 0;
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
    });
}

// ---------------------------------------------------------------------------
// The blocking beat body (background executor)
// ---------------------------------------------------------------------------

/// Everything the background beat needs, snapshotted on the foreground.
#[derive(Clone)]
struct BeatSnapshot {
    trpc: Arc<api::TrpcClient>,
    device_id: String,
    settings_path: PathBuf,
    repos_root: PathBuf,
    branch_prefix: String,
    held_branches: HashSet<String>,
    active_sessions: u32,
    report_requested: bool,
}

fn snapshot_for(account_id: &str, cx: &mut App) -> Option<BeatSnapshot> {
    let account = queries::active_account(cx)?;
    if account.id != account_id {
        return None;
    }
    let trpc = Arc::new(queries::trpc_client(cx)?);
    let auth = crate::session::AuthContext::global(cx);
    let device_id = steer::persistent_device_id(&auth.data_dir);
    let settings_path = coding::Settings::default_path(&auth.data_dir);
    let sessions = LocalSessions::global(cx);
    let sessions = sessions.read(cx);
    let held_branches: HashSet<String> =
        sessions.held_branches().map(str::to_string).collect();
    let active_sessions = sessions.session_ids().len() as u32;
    let settings = CodingHub::global(cx).read(cx).settings.clone();
    Some(BeatSnapshot {
        trpc,
        device_id,
        repos_root: settings.repos_root_path(),
        branch_prefix: settings.branch_prefix.clone(),
        settings_path,
        held_branches,
        active_sessions,
        report_requested: false,
    })
}

struct BeatOutcome {
    inventory_fp: Option<u64>,
    defaults_changed: bool,
}

/// One heartbeat + work pull + (when due) inventory report. Blocking.
fn beat(
    snapshot: &BeatSnapshot,
    scan_due: bool,
    last_fp: Option<u64>,
    worker_busy: &Arc<AtomicBool>,
) -> BeatOutcome {
    let mut last_fp = last_fp;
    let mut defaults_changed = false;
    let synced_at = coding::read_marker(&snapshot.settings_path, &snapshot.device_id).synced_at;
    match api::devices::heartbeat(
        &snapshot.trpc,
        &snapshot.device_id,
        snapshot.active_sessions,
        synced_at.as_deref(),
    ) {
        Ok(result) => {
            // `ok: false` (row removed) heals via the control channel's own
            // register on its next re-dial; the beat keeps running.
            if result.launch_defaults.is_some() || result.launch_defaults_updated_at.is_some() {
                defaults_changed = reconcile_server_defaults(
                    snapshot,
                    result.launch_defaults.as_ref(),
                    result.launch_defaults_updated_at.as_deref(),
                );
            }
            if !result.commands.is_empty() {
                // One batch at a time — a long prune must not pile up.
                if worker_busy
                    .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
                    .is_ok()
                {
                    for command in &result.commands {
                        run_device_command(snapshot, command);
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
    }
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
            None => {
                push_defaults(snapshot, &settings, &marker);
                false
            }
        },
        coding::ReconcileAction::PushLocal => {
            push_defaults(snapshot, &settings, &marker);
            false
        }
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

fn push_defaults(
    snapshot: &BeatSnapshot,
    settings: &coding::Settings,
    marker: &coding::SyncMarker,
) {
    let wire = serde_json::to_value(coding::defaults_wire(settings))
        .expect("defaults serialize cannot fail");
    let expected = api::devices::ExpectedStamp::Expect(marker.synced_at.as_deref());
    match api::devices::set_launch_defaults(&snapshot.trpc, &snapshot.device_id, &wire, expected) {
        Ok(result) if result.conflict => {
            log::info!("[device-sync] defaults push conflicted — adopting the server copy");
            if let Some(value) = result.launch_defaults.as_ref() {
                apply_server_defaults(
                    snapshot,
                    value,
                    result.launch_defaults_updated_at.as_deref(),
                );
            }
        }
        Ok(result) => {
            let marker = coding::SyncMarker {
                synced_at: result.launch_defaults_updated_at,
                dirty: false,
                hash: Some(coding::defaults_fingerprint(settings)),
            };
            let _ = coding::write_marker(&snapshot.settings_path, &snapshot.device_id, &marker);
        }
        Err(err) => {
            log::debug!("[device-sync] defaults push failed ({err}) — queued");
            let queued = coding::SyncMarker {
                dirty: true,
                synced_at: marker.synced_at.clone(),
                hash: marker.hash.clone(),
            };
            let _ = coding::write_marker(&snapshot.settings_path, &snapshot.device_id, &queued);
        }
    }
}

/// EXP-481: called by `CodingHub::save_settings` AFTER a successful save —
/// push the launch defaults up when they changed (offline queues via the
/// dirty marker; the beat retries). Background-executed by the caller.
pub(crate) fn push_local_defaults_if_changed(
    trpc: api::TrpcClient,
    settings_path: PathBuf,
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
        settings_path,
        repos_root: settings.repos_root_path(),
        branch_prefix: settings.branch_prefix.clone(),
        held_branches: HashSet::new(),
        active_sessions: 0,
        report_requested: false,
    };
    push_defaults(&snapshot, &settings, &marker);
}

// ---------------------------------------------------------------------------
// Commands + inventory (the daemon's run_device_command twin)
// ---------------------------------------------------------------------------

fn run_device_command(snapshot: &BeatSnapshot, command: &api::devices::PendingCommand) {
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
    if let Err(err) =
        api::devices::complete_command(&snapshot.trpc, &command.id, ok, Some(&message))
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
