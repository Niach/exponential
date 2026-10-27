//! `exponential daemon` — register this machine as a persistent per-user
//! device and run the sessions queued for it: the headless twin of the
//! desktop's session host. One control channel to the relay (presence,
//! check-in nudges and transcript serving — dialed only while at least one
//! agent CLI is installed, EXP-367), `devices.register` + periodic heartbeat
//! for the durable registry row, and (EXP-1292) the session pickup: the
//! database is the bus — the daemon's own `sessions` / `messages` shapes
//! carry every start and message, and the same `coding::pickup` path
//! `code`/`run` use claims and runs them. `daemon install|uninstall|status`
//! manage a systemd user unit (Linux) / launchd agent (macOS).

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{bail, Context as _};
use domain::rows::{MessageRow, SessionRow};
use coding::LaunchOptions;
use steer::{ControlApi, DeviceIdentity, TrpcControlApi};

use super::{reject_unknown_flags, take_flag, take_value, CommandResult};
use crate::context::{self, Ctx};
use crate::launch;
use crate::registry;
use crate::session_host::RunningSession;

/// EXP-481: 30s (down from 60) — online-ness now derives from
/// `last_seen_at` freshness against the contract's 90s window, so one
/// missed beat must not flap the badge.
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(30);
/// EXP-681 (EXP-1292: ported from the deleted kill poll): how long a 426
/// gate must hold before the daemon suspends its live sessions itself — the
/// server's staleness window plus its sweep cadence, so a gate that lifts
/// sooner (a rolled-back deploy) never touches a working run.
const GATED_SUSPEND_AFTER: Duration =
    Duration::from_millis(domain::contract::CODING_SESSION_STALE_MS as u64 + 30 * 60 * 1000);

/// EXP-641: how often a 426-gated daemon re-tries the self-update while the
/// gate holds and no newer release was installable yet (the release assets
/// can still be uploading when the web deploy that raised the floor lands).
const GATED_UPDATE_RETRY: Duration = Duration::from_secs(5 * 60);

/// FEED-36: a pending update no longer waits for the LAST session to end
/// — an attended run (a Chat, a web start) never ends on its own on a
/// headless server (EXP-674), so one forgotten chat parked every update
/// forever. Once EVERY live session has sat idle (between turns, no tool
/// running) for this long, the daemon ends them and applies the update;
/// repo-backed runs stay resumable. The web's "Update now" skips the wait.
pub const UPDATE_IDLE_GRACE: Duration = Duration::from_secs(2 * 60 * 60);
/// EXP-1353: how often the loop offers every clone to the shared cargo
/// target GC (its own throttle walks each at most once an hour).
const CARGO_TARGET_GC_BEAT: Duration = Duration::from_secs(5 * 60);
const DOCTOR_RECHECK: Duration = Duration::from_secs(5 * 60);
/// EXP-414: a changed agent advertisement is only ACTED on once a second
/// probe agrees ([`advert_transition`]) — this is the shortened recheck that
/// confirms (or clears) a pending change, so a real change still converges
/// in ~DOCTOR_RECHECK + this instead of two full periods.
const ADVERT_CONFIRM_RECHECK: Duration = Duration::from_secs(30);
/// EXP-746 (D9): the caps this daemon advertises — ONE list, owned by
/// [`coding::doctor::DEVICE_CAPS`] + [`coding::doctor::RUN_CAPS`] and
/// shared with the desktop (`ui::steer_wiring`). They used to be hand-synced
/// copies here and there, and a one-sided edit silently made one host
/// un-targetable for the new feature.
fn device_caps(advertised: &coding::AgentAdvertisement) -> Vec<String> {
    coding::device_caps(advertised)
}

// ---------------------------------------------------------------------------
// Signal handling — shared with `code`'s detached wait.
// ---------------------------------------------------------------------------

static SHUTDOWN: AtomicBool = AtomicBool::new(false);

extern "C" fn on_signal(_signal: libc::c_int) {
    SHUTDOWN.store(true, Ordering::SeqCst);
}

pub fn install_signal_handler() {
    let handler = on_signal as extern "C" fn(libc::c_int) as *const () as libc::sighandler_t;
    unsafe {
        libc::signal(libc::SIGINT, handler);
        libc::signal(libc::SIGTERM, handler);
    }
}

pub fn shutdown_requested() -> bool {
    SHUTDOWN.load(Ordering::SeqCst)
}

// ---------------------------------------------------------------------------
// Pidfile
// ---------------------------------------------------------------------------

fn pidfile(data_dir: &Path) -> PathBuf {
    data_dir.join("cli-daemon.pid")
}

/// The last `--label` value this machine actually applied via
/// `devices.rename`. `install --label` bakes the flag into the service's
/// ExecStart, so without this latch every daemon restart (reboot,
/// auto-update re-exec) would replay the one-time install intent and
/// silently revert a rename made in the web UI's machine list.
fn applied_label_file(data_dir: &Path) -> PathBuf {
    data_dir.join("cli-daemon.label")
}

/// The running daemon's pid, liveness- AND identity-checked. The pidfile
/// survives an unclean shutdown (power loss, OOM SIGKILL, hard reboot), and
/// after a reboot the recorded pid can belong to ANY live same-user process
/// — `kill(pid, 0)` alone then blocks startup forever (and lets `uninstall`
/// signal an innocent process), so a live pid only counts when it still
/// looks like our binary.
pub fn daemon_pid(data_dir: &Path) -> Option<u32> {
    let raw = std::fs::read_to_string(pidfile(data_dir)).ok()?;
    let pid: u32 = raw.trim().parse().ok()?;
    let alive = unsafe { libc::kill(pid as i32, 0) } == 0;
    (alive && pid_is_exponential(pid)).then_some(pid)
}

/// Best-effort process-identity probe: does `pid` run an executable named
/// `exponential`? Fails OPEN (true) when the identity cannot be read — a
/// pid racing its own exit or a /proc-less mount must keep the conservative
/// "a daemon is already running" behavior, never yield two daemons.
fn pid_is_exponential(pid: u32) -> bool {
    #[cfg(target_os = "linux")]
    {
        // /proc/<pid>/comm is the executable name truncated to 15 bytes —
        // "exponential" (11) fits whole.
        match std::fs::read_to_string(format!("/proc/{pid}/comm")) {
            Ok(comm) => comm.trim() == "exponential",
            Err(_) => true,
        }
    }
    #[cfg(target_os = "macos")]
    {
        let mut buf = [0u8; libc::PROC_PIDPATHINFO_MAXSIZE as usize];
        let len = unsafe {
            libc::proc_pidpath(
                pid as libc::c_int,
                buf.as_mut_ptr() as *mut libc::c_void,
                buf.len() as u32,
            )
        };
        if len <= 0 {
            return true;
        }
        let path = String::from_utf8_lossy(&buf[..len as usize]).into_owned();
        Path::new(&path)
            .file_name()
            .is_none_or(|name| name == "exponential")
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        let _ = pid;
        true
    }
}

// ---------------------------------------------------------------------------
// Dispatch
// ---------------------------------------------------------------------------

pub fn run(args: &[String]) -> CommandResult {
    match args.first().map(String::as_str) {
        Some("install") => install(&args[1..]),
        Some("uninstall") => uninstall(&args[1..]),
        Some("status") => status(&args[1..]),
        _ => run_daemon(args),
    }
}

// ---------------------------------------------------------------------------
// The daemon loop
// ---------------------------------------------------------------------------

/// Why an update is parked for the next idle moment.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum UpdateTrigger {
    /// The settings-gated periodic check came due.
    Scheduled,
    /// The web "Update" button (heartbeat `updateRequested`) — acts even
    /// with auto-update off, and consumes the request either way.
    Requested,
    /// EXP-641: the server 426-gated this build (sync, heartbeat or control
    /// channel). A gated daemon is dead weight — sync stopped, heartbeats
    /// rejected, so even the web "Update" button (which rides the heartbeat
    /// RESPONSE) can no longer reach it — and its only way back is the new
    /// binary. Acts like `Requested` (auto-update off is not a reason to
    /// stay unusable) and bypasses the persisted check throttle; re-armed
    /// every [`GATED_UPDATE_RETRY`] while the gate holds. Like every
    /// trigger it still waits for idle — FEED-36's [`UPDATE_IDLE_GRACE`]
    /// bounds that, so a forgotten idle session cannot pin a gated build.
    Gated,
}

/// EXP-641: whether a gated daemon should (re-)arm the update now. Pure so
/// the retry cadence is unit-testable: the first attempt is immediate, later
/// ones wait [`GATED_UPDATE_RETRY`] from the previous arm.
fn gated_update_due(gated: bool, last_attempt: Option<Instant>, now: Instant) -> bool {
    gated
        && last_attempt
            .is_none_or(|last| now.saturating_duration_since(last) >= GATED_UPDATE_RETRY)
}

/// One live session the daemon supervises (the desktop's `LocalSessions`).
/// EXP-1292: what the session is ABOUT lives on its `sessions` row; the
/// daemon keeps only what its own duties read.
struct LiveSession {
    branch: String,
    /// EXP-637: the run's own worktree, reclaimed by the reaper when the
    /// session finishes clean. `None` for an issue's session (its worktree
    /// survives by design) and repo-less runs.
    cleanup: Option<coding::RunCleanup>,
    session: Arc<RunningSession>,
    /// FEED-36: since when the agent has been between turns (`None` while a
    /// turn is in flight) — the loop's 1 Hz tick keeps it; a pending update
    /// reads it through [`update_gate`].
    idle_since: Option<Instant>,
}

type Sessions = Arc<Mutex<Vec<LiveSession>>>;

/// FEED-36: what a pending update may do given the live sessions' idle
/// stretches (`None` = a turn is in flight).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum UpdateGate {
    /// No live session — install and re-exec now.
    Apply,
    /// Every live session has idled past the grace — end them, then apply.
    EndIdle,
    /// At least one session is working (or idle for less than the grace).
    Wait,
}

fn update_gate(idle_for: impl IntoIterator<Item = Option<Duration>>, grace: Duration) -> UpdateGate {
    let mut any = false;
    for idle in idle_for {
        any = true;
        match idle {
            Some(idle) if idle >= grace => {}
            _ => return UpdateGate::Wait,
        }
    }
    if any {
        UpdateGate::EndIdle
    } else {
        UpdateGate::Apply
    }
}

fn lock_sessions(sessions: &Sessions) -> std::sync::MutexGuard<'_, Vec<LiveSession>> {
    match sessions.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

fn run_daemon(args: &[String]) -> CommandResult {
    let mut args = args.to_vec();
    let label_flag = take_value(&mut args, "--label");
    // Always runs in the foreground — systemd/launchd own daemonization.
    let _ = take_flag(&mut args, "--foreground");
    reject_unknown_flags(&args)?;

    let ctx = Arc::new(context::load()?);
    if let Some(pid) = daemon_pid(&ctx.data_dir) {
        // Same pid = US, after an auto-update re-exec (exec keeps the pid
        // and the pidfile) — that's a restart, not a second daemon.
        if pid != std::process::id() {
            bail!("A daemon is already running (pid {pid}).");
        }
    }
    std::fs::write(pidfile(&ctx.data_dir), std::process::id().to_string())
        .context("write the daemon pidfile")?;
    install_signal_handler();

    let device_id = ctx.device_id();
    let explicit_label = label_flag.filter(|label| !label.is_empty());
    let device_label = explicit_label
        .clone()
        .unwrap_or_else(api::users::hostname);
    log::info!(
        "daemon starting: {} as `{device_label}` ({device_id}) on {}",
        ctx.account.email,
        ctx.account.instance_url
    );

    // EXP-746: ONE registry decision for every end this process issues.
    // Installed before any session can launch (first caller wins,
    // process-wide).
    registry::install_end_observer(ctx.data_dir.clone());

    // EXP-757 (desktop parity): reclaim the scratch dirs of repo-less runs
    // nothing runs any more; a live sibling's (the desktop app on this
    // machine) are the keep set. Nothing is live in THIS process yet.
    sweep_scratch_dirs(&ctx);
    // EXP-773/EXP-886: apply the device's "Keep session history" window
    // (settings.json `sessionRetentionDays`, shared with the desktop app) to
    // stored transcripts and resume records. Unlimited, the default, keeps
    // everything.
    {
        let data_dir = ctx.data_dir.clone();
        std::thread::spawn(move || {
            steer::prune_session_history(&data_dir);
        });
    }
    // EXP-758: an ACP child outlives a host that died without its quit sweep
    // (a crash, SIGKILL, a hard reboot of the service), and nothing else ever
    // kills it, since the reaper's `claude-hooks` anchor only finds claude. The recorded pids are the anchor here, and a pid whose host is
    // still alive (the desktop app sharing this data dir, REV-20) is skipped.
    let orphans = coding::reaper::reap_recorded(&ctx.data_dir);
    if orphans > 0 {
        log::info!("reaped {orphans} orphaned agent process(es) from an earlier run");
    }

    let runtime = match steer::SteerRuntime::new() {
        Ok(runtime) => Some(runtime),
        Err(err) => {
            log::warn!("steer runtime failed to start (sessions cannot run here): {err}");
            None
        }
    };
    let personal_key = context::ensure_personal_key(&ctx).ok();
    let sessions: Sessions = Arc::new(Mutex::new(Vec::new()));
    // EXP-484: the collected agent status (accounts + usage windows), filled
    // OFF the 1Hz loop by the device worker and drained by the next
    // heartbeat. `collect_if_due` can block for ~10s (a codex app-server
    // spawn) — it must never sit on this loop.
    let agent_status: Arc<Mutex<Option<coding::AgentStatusPayload>>> = Arc::new(Mutex::new(None));
    // EXP-484: raised by a finished `agent_login` — the doctor re-probe (and
    // with it the accounts map) must not wait out a full DOCTOR_RECHECK
    // before the machine rows learn who just signed in.
    let doctor_soon = Arc::new(AtomicBool::new(false));
    // EXP-481: the serialized device-state worker (defaults convergence,
    // worktree commands, inventory reports) + the relay check_in flag that
    // forces an immediate heartbeat (the beat IS the work pull).
    // FEED-36: raised by the worker's `update_now` command once it ended
    // the live sessions; the loop arms the update off it.
    let update_now = Arc::new(AtomicBool::new(false));
    // EXP-484: the command ids of the `agent_login` PTYs and `agent_update`
    // downloads in flight on their own threads. Shared with the loop so a
    // daemon self-update never re-execs while `claude update` is mid-download.
    let logins_inflight: Arc<Mutex<HashSet<String>>> = Arc::new(Mutex::new(HashSet::new()));
    let check_in = Arc::new(AtomicBool::new(false));
    let device_worker = spawn_device_worker(
        Arc::clone(&ctx),
        Arc::clone(&sessions),
        device_id.clone(),
        Arc::clone(&agent_status),
        Arc::clone(&doctor_soon),
        Arc::clone(&update_now),
        Arc::clone(&logins_inflight),
        Arc::clone(&check_in),
    );
    // EXP-1292: the pickup host — this daemon's own Electric
    // pipeline (a shape subset in `sync-cli.sqlite`, never the GUI's store),
    // ONE delta drain nudging the ONE serialized worker, plus the 30s
    // self-tick below. Sync failing to open is not fatal for the device row
    // (heartbeat, worktrees, history keep working), but nothing reaches this
    // machine to run until it does.
    // EXP-641: raised by every path the server can 426 (the sync pipeline's
    // upgrade hook, the heartbeat) — the loop below turns it into an
    // immediate, throttle-free self-update attempt.
    let gated = Arc::new(AtomicBool::new(false));
    let sync_manager = start_daemon_sync(&ctx, &gated);
    let pickup = Arc::new(DaemonPickup {
        ctx: Arc::clone(&ctx),
        runtime: runtime.clone(),
        personal_key: personal_key.clone(),
        sessions: Arc::clone(&sessions),
        live: crate::launch::answer_tapped_registry(Arc::clone(&ctx.trpc)),
        sync: sync_manager.clone(),
        device_id: device_id.clone(),
        in_flight: Arc::new(Mutex::new(HashSet::new())),
        backoff: Mutex::new(HashMap::new()),
        reclaiming: Mutex::new(HashSet::new()),
        reclaim_until: Instant::now() + RECLAIM_GRACE,
    });
    // EXP-229 parity, NEW-5: the sessions a previous process of this install
    // left behind are RE-CLAIMED once the shapes sync ([`DaemonHost::
    // reconcile_rows`]: a mid-turn row continues, a pending message
    // replays, the rest idles) — never ended. Pid-guarded: rows owned by a
    // live sibling process are skipped. Without a sync pipeline nothing can
    // re-claim them, so they only go idle (the pre-NEW-5 rule).
    let reclaim = reconcile_stale_sessions(&ctx, sync_manager.is_some());
    lock_or_recover(&pickup.reclaiming).extend(reclaim);
    let daemon_worker = sync_manager.as_ref().map(|manager| {
        let synced = Arc::new(SyncedShapes::default());
        let worker = spawn_daemon_worker(Arc::clone(&pickup), Arc::clone(&synced));
        spawn_delta_drain(manager, synced, worker.clone());
        worker
    });

    let (mut advertised, mut doctor) = probe_agents(&ctx);
    maybe_fetch_codex(&ctx, &doctor_soon);
    // What the two jsonb columns last SENT said: a beat attaches a map only
    // when it actually changed, so the steady-state body stays tiny and
    // `agent_usage_at` (which the server stamps on every write) does not move
    // on every beat. Accounts compare on `accounts_key` — their IDENTITY —
    // because every collection pass restamps `checkedAt`, so a JSON compare
    // would call an unchanged map "changed" on every single beat.
    let mut sent_accounts: Option<String> = None;
    let mut sent_usage: Option<String> = None;
    // EXP-1196: the readiness block's items as last ACCEPTED by a beat
    // (`device_doctor::items_key`, `checkedAt` excluded).
    let mut sent_doctor: Option<String> = None;
    // EXP-1099: warn-level failure logging + the optional-payload back-off
    // (a 4xx on a beat carrying accounts/usage → the next goes out bare).
    let mut heartbeat_health = coding::logging::HeartbeatHealth::new();
    // EXP-414: a failed register (network not up yet at boot) is retried on
    // the heartbeat cadence — otherwise the registry row goes stale (old
    // version/agents, a never-cleared update request) until the next restart.
    let mut registered_ok = register_device(
        &ctx,
        &device_id,
        &device_label,
        &advertised,
        &doctor,
        &device_worker,
    );
    device_worker.send(DeviceWork::ReportWorktrees).ok();
    // `register` only SEEDS the label (it never stomps a rename); an
    // explicit --label is an intentional write and goes through `rename` —
    // but only when its VALUE changed since the last applied one, so a
    // service-baked flag doesn't replay on every restart and stomp a web
    // rename (a failed rename leaves the latch unwritten and retries on the
    // next start).
    if let Some(label) = &explicit_label {
        let latch = applied_label_file(&ctx.data_dir);
        let last_applied = std::fs::read_to_string(&latch).ok();
        if last_applied.as_deref().map(str::trim) != Some(label.as_str()) {
            match api::devices::rename(&ctx.trpc, &device_id, label) {
                Ok(()) => {
                    if let Err(err) = std::fs::write(&latch, label) {
                        log::debug!("persisting the applied --label failed: {err}");
                    }
                }
                Err(err) => log::debug!("devices.rename for --label failed: {err}"),
            }
        }
    }

    let mut control = runtime.as_ref().and_then(|runtime| {
        dial_control(runtime, &ctx, &device_id, &device_label, &advertised, &check_in)
    });
    if advertised.nothing_installed() {
        log::info!("no agent CLI installed — registered offline; install claude or codex to run sessions here");
    } else if advertised.agents.is_empty() {
        log::info!(
            "no agent CLI signed in ({} installed but signed out) — sessions will be refused until one is",
            advertised.unauthed_agents.join(", ")
        );
    }

    let mut last_heartbeat = Instant::now();
    let mut last_doctor = Instant::now();
    // Auto-update (EXP-403): a due check (own cadence, or the web "Update"
    // button via the heartbeat) parks here until NO session is live — a
    // restart must never kill a running agent. `Requested` acts even with
    // auto-update off (an explicit click is an explicit instruction). The
    // deferral is visible remotely: every heartbeat carries the live-session
    // count, so the machine rows read "Update queued" instead of spinning
    // (EXP-411).
    let mut pending_update: Option<UpdateTrigger> = None;
    let mut last_update_poll = Instant::now();
    // EXP-641: when the gated trigger was last armed (retry cadence).
    let mut last_gated_attempt: Option<Instant> = None;
    // EXP-681 (EXP-1292 port): since when the 426 gate holds, and whether
    // its live sessions were already suspended ([`GATED_SUSPEND_AFTER`]).
    let mut gated_since: Option<Instant> = None;
    let mut gated_suspended = false;
    // Logged once per hold: a pending update waiting on an in-flight
    // `agent_login`/`agent_update` command, not on a session.
    let mut update_held_by_claims = false;
    // EXP-414: an advertisement change observed by ONE probe, awaiting a
    // second agreeing probe before it tears the control channel down.
    let mut pending_advert: Option<coding::AgentAdvertisement> = None;
    // EXP-411: the live-session count last reported over the heartbeat. A
    // change forces an off-cadence beat so a session starting or ending
    // converges in ~1s (and a restarted daemon corrects a stale count on its
    // first tick) instead of up to a full heartbeat interval.
    let mut reported_sessions: Option<usize> = None;
    // EXP-530: the worker beat. The pickup rides the delta drain (it acts
    // within a second of the row landing); this tick is the catch-up pass
    // after a sleep/offline stretch, where no delta ever arrives.
    let mut last_worker_tick = Instant::now();
    // EXP-1005: the account-rotation wall beat (every ROTATION_BEAT).
    let mut rotation = RotationHost {
        ctx: Arc::clone(&ctx),
        sessions: Arc::clone(&sessions),
        pickup: Arc::clone(&pickup),
        // Persisted: an auto-update re-exec keeps every chain's cap.
        tracker: Arc::new(Mutex::new(coding::account_rotation::RotationTracker::load(
            &ctx.data_dir,
            chrono::Utc::now().timestamp_millis(),
        ))),
        inflight: Arc::new(Mutex::new(coding::account_rotation::InflightProbes::new())),
        holds: HoldLog::default(),
        last_beat: Instant::now(),
    };
    // EXP-1353: the desktop sweeps on its AutoSync prune; the daemon has no
    // trunk sync, so it offers its clones here. `None` = the first beat.
    let mut last_cargo_gc: Option<Instant> = None;
    let cargo_gc_running = Arc::new(AtomicBool::new(false));
    while !shutdown_requested() {
        // The 1 Hz loop: starts and messages arrive on the worker (shape
        // deltas), never here.
        std::thread::sleep(Duration::from_secs(1));

        // EXP-637 → NEW-5: a finished RUN's worktree ends
        // ([`coding::finish_session_worktree`]): removed, its unpushed work
        // parked on `origin/exp/leftover-<id8>` and named in the session's
        // conversation. The git + push run on a side thread, never the loop.
        // EXP-764: a finished repo-LESS thread has no worktree to judge — it
        // is purged whole: scratch dir, claude trust entries, run record and
        // steer journal. A team agent's is kept (EXP-1292: it only idles).
        {
            let mut guard = lock_sessions(&sessions);
            // EXP-1292: a SUSPENDED run (an account switch re-spawns the
            // same row right after) is no finished run — its worktree or
            // scratch dir is what the resume re-enters.
            let reaped: Vec<(String, Option<coding::RunCleanup>, PathBuf)> = guard
                .iter()
                .filter(|live| live.session.is_done() && !live.session.was_suspended())
                .map(|live| {
                    (
                        live.session.session_id.clone(),
                        live.cleanup.clone(),
                        live.session.worktree.clone(),
                    )
                })
                .collect();
            guard.retain(|live| !live.session.is_done());
            // FEED-53: what the runs still live here hold; the reaped ones
            // are already out. In-flight starts hold the clone's launch gate,
            // which the cleanup already respects.
            let live_holders = coding::run_cleanup::LiveHolders {
                branches: guard
                    .iter()
                    .map(|live| live.branch.clone())
                    .filter(|branch| !branch.is_empty())
                    .collect(),
                worktrees: guard.iter().map(|live| live.session.worktree.clone()).collect(),
            };
            drop(guard);
            // The moment the runs were seen to END: the git work below can
            // hold the loop for seconds, and a resume that re-enters a
            // scratch dir in that window keeps it (`scratch::purge`).
            let reaped_at = std::time::SystemTime::now();
            for (session_id, cleanup, worktree) in reaped {
                match cleanup {
                    Some(cleanup) => {
                        // The record outlives a removal: a resume re-creates
                        // the worktree on the recorded branch; the registry's
                        // TTL retires the record.
                        let live = live_holders.clone().with_registry(&ctx.data_dir, &session_id);
                        let trpc = Arc::clone(&ctx.trpc);
                        let spawned = std::thread::Builder::new()
                            .name("exp-run-finish".to_string())
                            .spawn(move || {
                                let verdict =
                                    coding::finish_session_worktree(&trpc, &session_id, &cleanup, &live);
                                log::info!(
                                    "run cleanup [{session_id}] on {}: {verdict:?}",
                                    cleanup.branch
                                );
                            });
                        if let Err(err) = spawned {
                            log::warn!("run cleanup: finish thread failed to spawn: {err}");
                        }
                    }
                    // EXP-1292: a team agent never ends — its scratch dir and
                    // record are what the next message or firing wakes it in.
                    None if coding::run_registry::get(&ctx.data_dir, &session_id)
                        .is_some_and(|record| record.kind.is_persistent()) => {}
                    None if coding::scratch::is_scratch_dir(&ctx.data_dir, &worktree) => {
                        let purged = coding::scratch::purge(
                            &ctx.data_dir,
                            &session_id,
                            &worktree,
                            reaped_at,
                        );
                        if purged {
                            steer::remove_journal(&ctx.data_dir, &session_id);
                        }
                        log::info!(
                            "scratch purge [{session_id}] {}: purged={purged}",
                            worktree.display()
                        );
                    }
                    None => {}
                }
            }
        }

        // FEED-36: keep each run's idle stretch (the update gate reads it).
        {
            let now = Instant::now();
            for live in lock_sessions(&sessions).iter_mut() {
                live.idle_since = match (live.session.is_idle(), live.idle_since) {
                    (true, Some(since)) => Some(since),
                    (true, None) => Some(now),
                    (false, _) => None,
                };
            }
        }
        // EXP-1005: walled runs rotate to another account between turns.
        rotation.tick(&doctor);
        // EXP-1353: the size walk can take seconds — never on this loop nor
        // the serialized device worker; one sweep thread at a time. NEW-5:
        // the nightly stale-worktree sweep rides the same beat (its own
        // throttle sweeps each clone once a day), shielding this host's
        // live runs.
        if last_cargo_gc.is_none_or(|at| at.elapsed() >= CARGO_TARGET_GC_BEAT)
            && !cargo_gc_running.swap(true, Ordering::SeqCst)
        {
            last_cargo_gc = Some(Instant::now());
            let repos_root = coding::Settings::load(&coding::Settings::default_path(&ctx.data_dir))
                .repos_root_path();
            let live = {
                let guard = lock_sessions(&sessions);
                coding::LiveHolders {
                    branches: guard
                        .iter()
                        .filter(|live| !live.session.is_done())
                        .map(|live| live.branch.clone())
                        .filter(|branch| !branch.is_empty())
                        .collect(),
                    worktrees: guard
                        .iter()
                        .filter(|live| !live.session.is_done())
                        .map(|live| live.session.worktree.clone())
                        .collect(),
                }
            };
            let data_dir = ctx.data_dir.clone();
            let trpc = Arc::clone(&ctx.trpc);
            let running = Arc::clone(&cargo_gc_running);
            std::thread::spawn(move || {
                sweep_cargo_targets(&repos_root);
                sweep_stale_worktrees(&repos_root, &data_dir, live, &trpc);
                running.store(false, Ordering::SeqCst);
            });
        }
        let live_now = lock_sessions(&sessions).len();
        let session_change = reported_sessions != Some(live_now);
        if session_change {
            // EXP-1158: a person's start just moved this machine's last used
            // login (the launcher's stamp) — the beat for the change waits
            // for a FRESH collection, which nudges it (`then_beat`), so the
            // row carries the moved `active` flag instead of the old one.
            reported_sessions = Some(live_now);
            device_worker
                .send(DeviceWork::CollectAgentStatus {
                    report: Box::new(doctor.clone()),
                    then_beat: true,
                })
                .ok();
        }
        // EXP-481: a relay check_in nudge means "the server persisted new
        // work" — beat NOW instead of on the cadence (the beat is the pull).
        let nudged = check_in.swap(false, Ordering::SeqCst);
        if last_heartbeat.elapsed() >= HEARTBEAT_INTERVAL || nudged {
            last_heartbeat = Instant::now();
            // Optimistic: a failed beat just waits for the next scheduled
            // tick instead of retrying at 1Hz while the network is down.
            reported_sessions = Some(live_now);
            let synced_at =
                coding::read_marker(&coding::Settings::default_path(&ctx.data_dir), &device_id)
                    .synced_at;
            // EXP-484: whatever the worker collected since the last beat.
            // A map rides only when it CHANGED — the server stamps
            // `agent_usage_at` on every write, and a stamp that moves every
            // 30s would be pure sync noise. "Changed" is the accounts
            // IDENTITY (`accounts_key`, `checked_at` excluded — the
            // collector restamps it every pass) and the usage JSON.
            let status = agent_status.lock().ok().and_then(|slot| slot.clone());
            let accounts_json = status.as_ref().and_then(|status| status.accounts_json());
            let usage_json = status.as_ref().and_then(|status| status.usage_json());
            let accounts_key = status
                .as_ref()
                .filter(|_| accounts_json.is_some())
                .map(|status| coding::agent_accounts::accounts_key(&status.accounts));
            let usage_text = usage_json.as_ref().map(|value| value.to_string());
            // EXP-1099: a bare beat (after a payload 4xx) carries none of
            // the two; nothing is recorded as sent, so they ride again.
            let include_optional = heartbeat_health.begin_beat();
            let send_accounts =
                include_optional && accounts_key.is_some() && accounts_key != sent_accounts;
            let send_usage = include_optional && usage_text.is_some() && usage_text != sent_usage;
            // EXP-1196: the readiness block rides only when its items moved.
            let device_doctor = {
                let settings =
                    coding::Settings::load(&coding::Settings::default_path(&ctx.data_dir));
                coding::device_doctor::current(&settings, &ctx.data_dir, &doctor)
            };
            let doctor_key = coding::device_doctor::items_key(&device_doctor);
            let send_doctor = include_optional && sent_doctor.as_ref() != Some(&doctor_key);
            let doctor_json = send_doctor
                .then(|| serde_json::to_value(&device_doctor).ok())
                .flatten();
            let carried_optional = send_accounts || send_usage || doctor_json.is_some();
            // NEW-5: the sessions this process runs — a busy row on this
            // device missing from it is a dead process to the server.
            let live_session_ids = pickup.heartbeat_live_ids();
            // EXP-1374: each engine's last activity (skipped when empty).
            let session_activity = pickup.heartbeat_activity();
            match api::devices::heartbeat(
                &ctx.trpc,
                &api::devices::HeartbeatInput {
                    device_id: &device_id,
                    active_sessions: live_now as u32,
                    defaults_synced_at: synced_at.as_deref(),
                    agent_accounts: send_accounts.then_some(accounts_json.as_ref()).flatten(),
                    agent_usage: send_usage.then_some(usage_json.as_ref()).flatten(),
                    doctor: doctor_json.as_ref(),
                    live_session_ids: live_session_ids.as_deref(),
                    session_activity: Some(&session_activity),
                },
            ) {
                Ok(result) => {
                    heartbeat_health.on_ok("devices.heartbeat", carried_optional);
                    // Only an ACCEPTED beat updates the last-sent copies: a
                    // failed one must resend on the next tick.
                    if send_accounts {
                        sent_accounts = accounts_key.clone();
                    }
                    if send_usage {
                        sent_usage = usage_text.clone();
                    }
                    if doctor_json.is_some() {
                        sent_doctor = Some(doctor_key.clone());
                    }
                    // EXP-641: a beat the server ACCEPTS means the gate is
                    // gone (a rolled-back floor, or we already updated past
                    // it) — clear it, or the daemon polls GitHub every 5 min
                    // and logs "still gated" until its next exec.
                    if gated.swap(false, Ordering::SeqCst) {
                        log::info!("devices.heartbeat accepted again — the min-version gate lifted");
                    }
                    // Row removed in the UI while we run, or an earlier
                    // register never landed (EXP-414) — re-register.
                    if !result.ok || !registered_ok {
                        registered_ok = register_device(
                            &ctx,
                            &device_id,
                            &device_label,
                            &advertised,
                            &doctor,
                            &device_worker,
                        );
                    }
                    if result.update_requested && pending_update.is_none() {
                        if live_now > 0 {
                            log::info!(
                                "update requested from the web — parked until {live_now} live session(s) close or sit idle for {}h",
                                UPDATE_IDLE_GRACE.as_secs() / 3600
                            );
                        } else {
                            log::info!("update requested from the web");
                        }
                        pending_update = Some(UpdateTrigger::Requested);
                    }
                    // EXP-481: the beat's work pull — commands + (on stamp
                    // mismatch) the authoritative launch defaults.
                    if !result.commands.is_empty() {
                        device_worker.send(DeviceWork::Commands(result.commands)).ok();
                    }
                    if result.launch_defaults.is_some()
                        || result.launch_defaults_updated_at.is_some()
                    {
                        device_worker
                            .send(DeviceWork::ServerDefaults {
                                defaults: result.launch_defaults,
                                stamp: result.launch_defaults_updated_at,
                            })
                            .ok();
                    } else if result.ok {
                        // EXP-490: no payload = the stamps matched — a queued
                        // dirty push (offline save) must still retry at beat
                        // cadence, not just the slow doctor tick.
                        device_worker.send(DeviceWork::ReconcileLocal).ok();
                    }
                }
                Err(api::ApiError::UpgradeRequired) => {
                    // EXP-641: the min-version gate. Not a transient — the
                    // loop below updates out of it.
                    if !gated.swap(true, Ordering::SeqCst) {
                        log::warn!("devices.heartbeat: HTTP 426 — the server no longer accepts this build");
                    }
                }
                Err(err) => heartbeat_health.on_err("devices.heartbeat", &err, carried_optional),
            }
            // EXP-484: collect for the NEXT beat, on the worker — never
            // here (a codex app-server probe blocks for seconds, and this
            // loop also owns the update gate and the session reaper).
            device_worker
                .send(DeviceWork::CollectAgentStatus {
                    report: Box::new(doctor.clone()),
                    then_beat: false,
                })
                .ok();
        }
        // Session start/end changes the inventory's busy flags.
        if session_change {
            device_worker.send(DeviceWork::ReportWorktrees).ok();
        }

        if let Some(worker) = &daemon_worker {
            if last_worker_tick.elapsed() >= DAEMON_TICK {
                last_worker_tick = Instant::now();
                worker.send(DaemonWork::Tick).ok();
            }
        }

        // FEED-36: an `update_now` command (web "Update now") ended every
        // session on the worker; arm the request right here instead of
        // waiting for the flag to ride the next beat.
        if update_now.swap(false, Ordering::SeqCst) && pending_update.is_none() {
            log::info!("update now (web) — applying as soon as the sessions close");
            pending_update = Some(UpdateTrigger::Requested);
        }
        // Scheduled auto-update check (settings-gated + persisted throttle).
        if pending_update.is_none() && last_update_poll.elapsed() >= Duration::from_secs(60) {
            last_update_poll = Instant::now();
            if super::update::auto_check_due(
                &ctx.data_dir,
                super::update::DAEMON_CHECK_INTERVAL_SECS,
            ) {
                pending_update = Some(UpdateTrigger::Scheduled);
            }
        }
        // EXP-681 (EXP-1292 port): under a SUSTAINED gate the shapes are
        // rejected too, so no Stop or end can reach a live run any more, and
        // a run that never idles would hold the update forever: past
        // [`GATED_SUSPEND_AFTER`] every live session is suspended (its row
        // idles once a build the server accepts reports it).
        if gated.load(Ordering::SeqCst) {
            let since = *gated_since.get_or_insert_with(Instant::now);
            if !gated_suspended && since.elapsed() >= GATED_SUSPEND_AFTER {
                gated_suspended = true;
                for live in lock_sessions(&sessions).iter() {
                    log::warn!(
                        "session {} ({}): gated for {}s — nothing remote can stop it any more, suspending it",
                        live.session.session_id,
                        live.branch,
                        GATED_SUSPEND_AFTER.as_secs()
                    );
                    live.session.suspend();
                }
            }
        } else {
            gated_since = None;
            gated_suspended = false;
        }
        // EXP-641: a 426-gated build updates NOW (no auto-update opt-in, no
        // persisted throttle — both would leave the daemon dead until the
        // next 6h tick), retrying while the gate holds.
        if pending_update.is_none()
            && gated_update_due(gated.load(Ordering::SeqCst), last_gated_attempt, Instant::now())
        {
            last_gated_attempt = Some(Instant::now());
            let live = lock_sessions(&sessions).len();
            if live > 0 {
                log::info!(
                    "server rejected this build (426) — updating once {live} live session(s) close or sit idle for {}h",
                    UPDATE_IDLE_GRACE.as_secs() / 3600
                );
            } else {
                log::info!("server rejected this build (426) — updating now");
            }
            pending_update = Some(UpdateTrigger::Gated);
        }
        if let Some(trigger) = pending_update {
            // FEED-36: no session, or only sessions idle past the grace —
            // the latter are ended (ended_by `client`; the reaper above
            // drops them next tick, then this arm applies). An `agent_login`
            // PTY or an `agent_update` download in flight holds the restart
            // the same way a live session does: a re-exec mid-download would
            // orphan the updater and never `completeCommand` its row. Re-checked
            // on the next tick.
            let updater_busy = logins_inflight
                .lock()
                .map(|claims| !claims.is_empty())
                .unwrap_or(false);
            let gate = if updater_busy {
                if !update_held_by_claims {
                    log::info!(
                        "a {trigger:?} update is waiting for an agent sign-in/update command to finish"
                    );
                    update_held_by_claims = true;
                }
                UpdateGate::Wait
            } else {
                update_held_by_claims = false;
                let now = Instant::now();
                let guard = lock_sessions(&sessions);
                update_gate(
                    guard.iter().map(|live| live.idle_since.map(|since| now.saturating_duration_since(since))),
                    UPDATE_IDLE_GRACE,
                )
            };
            if gate == UpdateGate::EndIdle {
                for live in lock_sessions(&sessions).iter() {
                    log::info!(
                        "suspending session {} ({}) — idle for {}h, a {trigger:?} update is waiting",
                        live.session.session_id,
                        live.branch,
                        live.idle_since.map(|since| since.elapsed().as_secs() / 3600).unwrap_or(0)
                    );
                    // EXP-1292: the process goes, the session stays — the row
                    // idles and the next message resumes the SAME row.
                    live.session.suspend();
                }
            }
            if gate == UpdateGate::Apply {
                pending_update = None;
                match super::update::check_and_install() {
                    Ok(super::update::UpdateOutcome::Updated { version }) => {
                        log::info!("updated to {version} — restarting the daemon");
                        if let Some(handle) = control.take() {
                            handle.stop();
                        }
                        // exec keeps the pid: the pidfile stays valid and the
                        // new binary's startup sees its own pid there.
                        super::update::exec_self();
                        // exec only returns on failure — keep running.
                    }
                    Ok(other) => {
                        log::info!("update check: {other:?}");
                        if trigger == UpdateTrigger::Gated {
                            log::warn!(
                                "still gated and no newer cli release is installable yet — retrying in {}s",
                                GATED_UPDATE_RETRY.as_secs()
                            );
                        }
                        if matches!(trigger, UpdateTrigger::Requested) {
                            // Consume the web request even when there was
                            // nothing to install.
                            registered_ok = register_device(
                                &ctx,
                                &device_id,
                                &device_label,
                                &advertised,
                                &doctor,
                                &device_worker,
                            );
                        }
                    }
                    Err(err) => {
                        log::warn!("update failed: {err:#}");
                        if trigger == UpdateTrigger::Gated {
                            log::warn!(
                                "still gated — retrying the update in {}s",
                                GATED_UPDATE_RETRY.as_secs()
                            );
                        }
                        if matches!(trigger, UpdateTrigger::Requested) {
                            registered_ok = register_device(
                                &ctx,
                                &device_id,
                                &device_label,
                                &advertised,
                                &doctor,
                                &device_worker,
                            );
                        }
                    }
                }
            }
        }

        // Re-advertise on toolchain changes (the desktop's
        // `refresh_device_advertisement`): installing the first agent brings
        // the machine online without a restart; removing the last hangs up.
        // Sign-in state rides the same probe (EXP-409): logging into claude
        // over ssh flips the machine runnable without a restart.
        // EXP-414: acted on only once TWO consecutive probes agree — a single
        // flaky auth probe was flapping the machine offline every 5 minutes.
        let doctor_due = if pending_advert.is_some() {
            ADVERT_CONFIRM_RECHECK
        } else {
            DOCTOR_RECHECK
        };
        // EXP-484: a finished `agent_login` re-probes NOW — the accounts
        // map (and the row's signed-in flip) is the whole point of the
        // command, and it must not wait out the slow cadence.
        if doctor_soon.swap(false, Ordering::SeqCst) || last_doctor.elapsed() >= doctor_due {
            last_doctor = Instant::now();
            // EXP-481: the slow cadence also picks up hand-edited
            // settings.json defaults and re-checks the inventory.
            device_worker.send(DeviceWork::ReconcileLocal).ok();
            device_worker.send(DeviceWork::ReportWorktrees).ok();
            let (agents, report) = probe_agents(&ctx);
            doctor = report;
            maybe_fetch_codex(&ctx, &doctor_soon);
            match advert_transition(&advertised, &agents, &mut pending_advert) {
                AdvertStep::Keep => {}
                AdvertStep::AwaitConfirmation => log::info!(
                    "agent advertisement change observed ({advertised:?} -> {agents:?}) — awaiting confirmation"
                ),
                AdvertStep::Apply => {
                    log::info!("agent advertisement changed: {advertised:?} -> {agents:?}");
                    // EXP-485: the online frame carries neither the agent
                    // lists nor the launch defaults any more, so a changed
                    // advertisement only has to reach the devices ROW. The
                    // socket is touched ONLY when the dial decision itself
                    // flips (EXP-367 `nothing_installed`) — re-dialing for a
                    // model tweak was a pointless presence gap on every
                    // machine list.
                    let redial = advertised.nothing_installed() != agents.nothing_installed();
                    advertised = agents;
                    if redial {
                        if let Some(handle) = control.take() {
                            handle.stop();
                        }
                        control = runtime.as_ref().and_then(|runtime| {
                            dial_control(
                                runtime, &ctx, &device_id, &device_label, &advertised, &check_in,
                            )
                        });
                    }
                    registered_ok = register_device(
                        &ctx,
                        &device_id,
                        &device_label,
                        &advertised,
                        &doctor,
                        &device_worker,
                    );
                }
            }
        }
    }

    // --- Quit sweep (desktop parity): hang up, kill live children (their
    // supervisors end the rows), reap PTY escapees, drop the pidfile. ------
    log::info!("daemon stopping");
    if let Some(handle) = control.take() {
        handle.stop();
    }
    // EXP-530: stop the shape threads before the session teardown so the
    // pipeline isn't long-polling while children die (the store stays on
    // disk — it is a cache, and the next start resumes from its offsets).
    if let Some(manager) = &sync_manager {
        manager.stop_all();
    }
    let live: Vec<Arc<RunningSession>> = lock_sessions(&sessions)
        .iter()
        .map(|entry| Arc::clone(&entry.session))
        .collect();
    // EXP-1292: a daemon stop (logout, reboot, `daemon stop`) is not a person
    // ending the work — every row idles, and the next message resumes it,
    // exactly as after a crash.
    for session in &live {
        session.suspend();
    }
    let deadline = Instant::now() + Duration::from_secs(5);
    for session in &live {
        let remaining = deadline.saturating_duration_since(Instant::now());
        let _ = session.wait_timeout(remaining.max(Duration::from_millis(50)));
    }
    let reaped = coding::reaper::reap(&ctx.data_dir);
    if reaped > 0 {
        log::info!("reaped {reaped} escaped agent processes");
    }
    // EXP-758: the 5s grace above is a bound, not a guarantee, and an engine
    // that did not finish its end sequence in time still owns a live ACP
    // child, and this process is about to stop being its parent. Same rule
    // as the startup pass: only pids this host recorded, never a sibling's.
    let orphans = coding::reaper::reap_recorded(&ctx.data_dir);
    if orphans > 0 {
        log::info!("reaped {orphans} agent process(es) that outlived their session");
    }
    // EXP-781: the same scratch reclaim startup does (EXP-757), now that the
    // reaps are through and nothing this daemon owns is live. Without it a
    // repo-less run stayed resumable — its record and scratch dir outliving
    // the run — until the NEXT start swept it, which for a machine that is
    // shut down for a while is a long time. Synchronous: the process exits on
    // the next line.
    sweep_scratch_dirs_now(&ctx);
    let _ = std::fs::remove_file(pidfile(&ctx.data_dir));
    Ok(ExitCode::SUCCESS)
}

/// One doctor pass → what the relay/registry hears, plus the report itself.
/// EXP-484: the accounts map and the usage collector both read the REPORT
/// (which binaries exist, who is signed in), so the daemon keeps the last
/// one beside the advertisement instead of re-probing for it.
fn probe_agents(ctx: &Ctx) -> (coding::AgentAdvertisement, coding::DoctorReport) {
    let settings = coding::Settings::load(&coding::Settings::default_path(&ctx.data_dir));
    let report = coding::run_doctor(&settings, &ctx.data_dir);
    let advertisement = report.agent_advertisement(&settings);
    (advertisement, report)
}

/// EXP-1232: a stored Codex login with no managed build in place → fetch
/// the pinned build now, off the loop, and re-probe when it lands (the row
/// reads `Downloading…` meanwhile). At most one fetch per process. A failed
/// fetch is retried from here on the doctor cadence once `should_fetch`
/// says the cooldown passed — `RETRY_CAP` times, then the row stays
/// `Download failed · Update` until a person acts.
fn maybe_fetch_codex(ctx: &Ctx, doctor_soon: &Arc<AtomicBool>) {
    let settings = coding::Settings::load(&coding::Settings::default_path(&ctx.data_dir));
    if !coding::managed_codex::should_fetch(&settings, &ctx.data_dir) {
        return;
    }
    let retrying = matches!(
        coding::managed_codex::state(&ctx.data_dir),
        coding::managed_codex::State::Failed(_)
    );
    let doctor_soon = Arc::clone(doctor_soon);
    let started = coding::managed_codex::fetch_in_background(ctx.data_dir.clone(), move |result| {
        match result {
            Ok(path) => log::info!("managed codex: fetched {}", path.display()),
            Err(reason) => log::warn!("managed codex: fetch failed: {reason}"),
        }
        doctor_soon.store(true, Ordering::SeqCst);
    });
    if started && retrying {
        log::info!("managed codex: retrying the failed download of the pinned build");
    } else if started {
        log::info!("managed codex: a Codex login is stored here — fetching the pinned build");
    }
}

/// What a doctor re-probe should do to the live advertisement (EXP-414).
#[derive(Debug, PartialEq)]
enum AdvertStep {
    Keep,
    AwaitConfirmation,
    Apply,
}

/// A single disagreeing probe is treated as wobble — only two CONSECUTIVE
/// probes agreeing on the same changed value tear the control channel down
/// (the stop + re-dial is a real presence gap on every machine list).
fn advert_transition(
    current: &coding::AgentAdvertisement,
    observed: &coding::AgentAdvertisement,
    pending: &mut Option<coding::AgentAdvertisement>,
) -> AdvertStep {
    if observed == current {
        *pending = None;
        return AdvertStep::Keep;
    }
    if pending.as_ref() == Some(observed) {
        *pending = None;
        return AdvertStep::Apply;
    }
    *pending = Some(observed.clone());
    AdvertStep::AwaitConfirmation
}

/// Best-effort `devices.register` — registered even with no agents so the
/// UI can show the machine offline with a reason; a failed register must
/// never break the daemon. Returns whether the register
/// landed, so the daemon can retry a failure on the heartbeat cadence
/// (EXP-414) instead of running on a stale row until the next restart.
fn register_device(
    ctx: &Ctx,
    device_id: &str,
    device_label: &str,
    advertised: &coding::AgentAdvertisement,
    doctor: &coding::DoctorReport,
    device_worker: &flume::Sender<DeviceWork>,
) -> bool {
    let caps = device_caps(advertised);
    // EXP-481: the local defaults ride the register as a FIRST-EVER seed
    // (the server applies them only while its column is NULL); the response
    // carries the authoritative copy either way, reconciled off-loop.
    let settings = coding::Settings::load(&coding::Settings::default_path(&ctx.data_dir));
    let launch_defaults = serde_json::to_value(coding::defaults_wire(&settings))
        .expect("defaults serialize cannot fail");
    let accounts =
        doctor.agent_accounts_with_profiles(&settings, &ctx.data_dir, &coding::now_iso());
    let agent_accounts = (!accounts.is_empty())
        .then(|| serde_json::to_value(&accounts).ok())
        .flatten();
    let device_doctor =
        serde_json::to_value(coding::device_doctor::current(&settings, &ctx.data_dir, doctor)).ok();
    let result = api::devices::register(
        &ctx.trpc,
        &api::devices::RegisterDevice {
            device_id,
            label: device_label,
            kind: "server",
            platform: Some(std::env::consts::OS),
            agents: &advertised.agents,
            unauthed_agents: &advertised.unauthed_agents,
            // EXP-749: the ACP-ready subset, sent even when empty.
            acp_agents: Some(&advertised.acp_agents),
            caps: &caps,
            launch_defaults: Some(&launch_defaults),
            // EXP-484 (A3): who is signed in where, straight off the last
            // doctor pass — the usage windows are a heartbeat concern (they
            // need the collector), the accounts map is not, and a
            // just-registered machine should already say "signed in as …".
            agent_accounts: agent_accounts.as_ref(),
            version: Some(crate::cli_version()),
            // EXP-1196: the readiness block, always on register.
            doctor: device_doctor.as_ref(),
        },
    );
    match result {
        Ok(result) => {
            if result.launch_defaults.is_some() || result.launch_defaults_updated_at.is_some() {
                device_worker
                    .send(DeviceWork::ServerDefaults {
                        defaults: result.launch_defaults,
                        stamp: result.launch_defaults_updated_at,
                    })
                    .ok();
            }
            true
        }
        Err(err) => {
            // EXP-495: a 4xx is the server REJECTING this build's payload —
            // the machine will be missing from the web UI even though steer
            // presence reads online, so say that loudly; a transport error
            // stays a warning and retries.
            match &err {
                api::ApiError::Http { status: 400..=499, .. } => log::error!(
                    "devices.register rejected ({err}) — this machine will not appear in the web UI until a register succeeds (retrying on the heartbeat cadence)"
                ),
                _ => log::warn!("devices.register failed (transport error): {err}"),
            }
            false
        }
    }
}

/// EXP-367: never dial with NOTHING installed (the relay defaults an absent
/// agents list to ["claude"]). An installed-but-signed-out agent (EXP-409)
/// still dials — with an EXPLICIT empty runnable list — so the machine list
/// can say "sign in on that machine" instead of showing it offline.
fn dial_control(
    runtime: &Arc<steer::SteerRuntime>,
    ctx: &Ctx,
    device_id: &str,
    device_label: &str,
    advertised: &coding::AgentAdvertisement,
    check_in: &Arc<AtomicBool>,
) -> Option<steer::ControlChannelHandle> {
    if advertised.nothing_installed() {
        return None;
    }
    // EXP-672: presence only — the label, the agent lists, the launch
    // defaults and the caps every start gates on reach the server through
    // `devices.register`'s persisted row.
    let device = DeviceIdentity {
        device_id: device_id.to_string(),
        device_label: device_label.to_string(),
    };
    // Non-blocking by contract: just flip the flag — the 1s loop beats.
    let check_in = Arc::clone(check_in);
    let on_check_in: steer::control_channel::CheckInFn = Arc::new(move || {
        check_in.store(true, Ordering::SeqCst);
    });
    // EXP-773: serve this machine's stored transcript back to the relay. The
    // guard is per SOCKET, which is all it has to be: a redial is rare and a
    // relay only asks once per viewer join.
    let history_runtime = Arc::clone(runtime);
    let history_trpc = Arc::clone(&ctx.trpc);
    let history_dir = ctx.data_dir.clone();
    let history_in_flight = steer::HistoryInFlight::new();
    let on_history_request: steer::HistoryRequestFn = Arc::new(move |session_id: String| {
        let tickets: Arc<dyn steer::PublisherTickets> = Arc::new(steer::TrpcPublisherTickets {
            trpc: Arc::clone(&history_trpc),
            session_id: session_id.clone(),
        });
        steer::serve_history_request(
            &history_runtime,
            tickets,
            history_dir.clone(),
            session_id,
            history_in_flight.clone(),
        );
    });
    // EXP-796: "Load earlier" on a run this machine already replayed — the
    // page comes off the same journal, back down the control socket.
    let page_runtime = Arc::clone(runtime);
    let page_dir = ctx.data_dir.clone();
    let on_history_page: steer::HistoryPageFn = Arc::new(move |ask, reply| {
        steer::serve_history_page(&page_runtime, page_dir.clone(), ask, reply);
    });
    let control_api: Arc<dyn ControlApi> = Arc::new(TrpcControlApi(Arc::clone(&ctx.trpc)));
    Some(steer::spawn_control_channel(
        runtime,
        device,
        control_api,
        on_check_in,
        on_history_request,
        on_history_page,
        // NEW-55: the daemon is never a voice device (the voice runs in the
        // IDE alone), so a ring is not its to answer.
        Arc::new(|_ring| {}),
    ))
}

/// EXP-757: the startup pass of [`coding::scratch::sweep`], on its own
/// thread (a dir walk plus a `~/.claude.json` rewrite — nothing the daemon
/// loop should wait on).
fn sweep_scratch_dirs(ctx: &Arc<Ctx>) {
    let ctx = Arc::clone(ctx);
    std::thread::spawn(move || sweep_scratch_dirs_now(&ctx));
}

/// [`sweep_scratch_dirs`] on the CALLING thread — the quit sweep's shape.
/// There is no next loop iteration to hand it to, and a detached thread would
/// simply die with the process before it finished the walk.
fn sweep_scratch_dirs_now(ctx: &Ctx) {
    let live = registry::live_ids(&ctx.data_dir);
    let report = coding::scratch::sweep(&ctx.data_dir, &live);
    // EXP-764: the purged runs' steer journals go with their records.
    for session_id in &report.purged {
        steer::remove_journal(&ctx.data_dir, session_id);
    }
    if !report.is_noop() {
        log::info!(
            "scratch sweep: removed {} run dir(s), purged {} run(s), dropped {} trust entr(y/ies), kept {} live + {} young",
            report.removed.len(),
            report.purged.len(),
            report.trust_dropped,
            report.kept_live,
            report.kept_young
        );
    }
}

/// EXP-229 parity, EXP-1292 semantics: the sessions a previous process of
/// this install left behind are never `ended` — the SAME row resumes from
/// its run record (a crash, a suspend whose idle report failed; a team agent
/// always idles). Only a thread whose final `ended` report never resolved
/// (its run record says so) gets that END again.
///
/// NEW-5: with `reclaim` (the daemon syncs its shapes) the rest is returned
/// instead of idled — [`DaemonHost::reconcile_rows`] re-claims them from
/// their synced rows (a mid-turn one gets a continuation turn), and the
/// heartbeat counts them live until it has. Their registry entries are
/// dropped here: a re-claimed run records itself afresh under this pid.
fn reconcile_stale_sessions(ctx: &Ctx, reclaim: bool) -> HashSet<String> {
    let stale = registry::stale_entries(&ctx.data_dir, &ctx.account.id);
    let mut reclaimed = HashSet::new();
    if stale.is_empty() {
        return reclaimed;
    }
    let trpc = Arc::clone(&ctx.trpc);
    let data_dir = ctx.data_dir.clone();
    let owed_end = |id: &str| {
        coding::run_registry::get(&data_dir, id)
            .is_some_and(|record| !record.kind.is_persistent() && record.unreported_end())
    };
    let stale: Vec<registry::StaleEntry> = stale
        .into_iter()
        .filter(|entry| {
            let take = reclaim && (entry.crashed || !owed_end(&entry.id));
            if take {
                log::info!("reclaim: session {} was left behind by an earlier process — re-claiming it", entry.id);
                registry::remove(&data_dir, &entry.id);
                reclaimed.insert(entry.id.clone());
            }
            !take
        })
        .collect();
    if stale.is_empty() {
        return reclaimed;
    }
    std::thread::spawn(move || {
        for entry in stale {
            let owed_end = coding::run_registry::get(&data_dir, &entry.id).is_some_and(|record| {
                !record.kind.is_persistent() && record.unreported_end()
            });
            if entry.crashed || !owed_end {
                let result = api::session_device::report(&trpc, &entry.id, &idle_report());
                registry::apply_end_outcome(&data_dir, &entry.id, &result);
            } else if coding::end_session(&trpc, &entry.id).is_ok() {
                // The end observer applied the outcome to the registry.
                coding::run_registry::update(&data_dir, &entry.id, |record| {
                    record.set_unreported_end(false);
                    true
                });
            }
        }
    });
    reclaimed
}

// ---------------------------------------------------------------------------
// EXP-1005: the account-rotation wall beat
// ---------------------------------------------------------------------------

/// How often the loop reads its live runs' usage walls. The switch itself
/// waits for the turn slot, so a finer beat would buy nothing.
const ROTATION_BEAT: Duration = Duration::from_secs(5);

/// One live run's wall in the rotation's vocabulary — `None` unless the
/// wall is a rate limit. `account` = the run registry's recorded login (a
/// legacy record names none); no record at all is a run a switch could not
/// resume (a repo-less scratch run with a record rotates, EXP-1292).
fn walled_run(
    session_id: &str,
    worktree: &Path,
    agent: coding::CodingAgent,
    blocked: &steer::SessionBlocked,
    record: Option<&coding::run_registry::RunRecord>,
    idle: bool,
) -> Option<coding::account_rotation::WalledRun> {
    (blocked.kind == "rate_limit").then(|| coding::account_rotation::WalledRun {
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
        unrecorded: record.is_none(),
    })
}

/// The log-once key of a hold: its variant, never its timestamps (a
/// cooldown's `until_ms` does not move, but a re-parked wait's does).
fn hold_key(hold: &coding::account_rotation::Hold) -> &'static str {
    use coding::account_rotation::Hold;
    match hold {
        Hold::Off => "off",
        Hold::AgentNeverRotates => "agent_never_rotates",
        Hold::NoRecord => "no_record",
        Hold::MidTurn => "mid_turn",
        Hold::CoolingDown { .. } => "cooling_down",
        Hold::Capped => "capped",
        Hold::WaitingForReset { .. } => "waiting_for_reset",
        Hold::RetryingSwitch { .. } => "retrying_switch",
    }
}

/// Which hold each walled session was last logged under, so a hold is
/// logged once per change instead of every beat.
#[derive(Default)]
struct HoldLog(HashMap<String, &'static str>);

impl HoldLog {
    /// Record `key` for `session_id`; `true` = it changed (log it).
    fn changed(&mut self, session_id: &str, key: &'static str) -> bool {
        self.0.insert(session_id.to_string(), key) != Some(key)
    }

    fn clear(&mut self, session_id: &str) {
        self.0.remove(session_id);
    }

    /// Forget sessions no longer walled.
    fn retain(&mut self, walled: &HashSet<String>) {
        self.0.retain(|session_id, _| walled.contains(session_id));
    }
}

/// The daemon's half of EXP-1005: every [`ROTATION_BEAT`] it reads each live
/// run's wall, asks the ONE [`coding::account_rotation::RotationTracker`]
/// what to do, and — on a probe — spends the forced usage read OFF the loop
/// (one HTTPS GET per profile), then switches the run ([`switch_account`]:
/// the SAME row re-spawned natively under the target, EXP-1292) or parks it.
/// EXP-1107: ONE read per AGENT per beat, every walled run of that agent
/// decided off it (`plan_beat` / `decide_batch`); a switch that fails or is
/// skipped is undone (`undo_switch`).
struct RotationHost {
    ctx: Arc<Ctx>,
    sessions: Sessions,
    /// EXP-1292: a switch relaunches through the pickup's collaborators and
    /// holds the session in its in-flight set, so the pass never acts on it
    /// mid-switch.
    pickup: Arc<DaemonPickup>,
    tracker: Arc<Mutex<coding::account_rotation::RotationTracker>>,
    /// The agents with a probe batch (and its switches) in flight: never
    /// probed twice, their chains kept in the tracker while a switch has
    /// ended the old run but not yet registered the new one.
    inflight: Arc<Mutex<coding::account_rotation::InflightProbes>>,
    holds: HoldLog,
    last_beat: Instant,
}

impl RotationHost {
    fn tick(&mut self, doctor: &coding::DoctorReport) {
        if self.last_beat.elapsed() < ROTATION_BEAT {
            return;
        }
        self.last_beat = Instant::now();
        self.beat(doctor);
    }

    fn beat(&mut self, doctor: &coding::DoctorReport) {
        let (live_chains, candidates) = {
            let guard = lock_sessions(&self.sessions);
            let live_chains: Vec<String> = guard
                .iter()
                .map(|live| live.session.worktree.to_string_lossy().into_owned())
                .collect();
            let candidates: Vec<_> = guard
                .iter()
                .filter(|live| !live.session.is_done())
                .filter_map(|live| {
                    let blocked = live.session.blocked()?;
                    Some((
                        live.session.session_id.clone(),
                        live.session.worktree.clone(),
                        live.session.coding_agent(),
                        blocked,
                        live.session.is_idle(),
                    ))
                })
                .collect();
            (live_chains, candidates)
        };
        let now_ms = chrono::Utc::now().timestamp_millis();
        let inflight = lock_or_recover(&self.inflight).clone();
        let mut keep = live_chains;
        keep.extend(inflight.chains());
        // Grace-based: a chain between an ended row and its resumed
        // successor keeps its cooldown and cap (see `CHAIN_GRACE_MS`).
        lock_or_recover(&self.tracker).retain_chains(&keep, now_ms);

        let walled: Vec<_> = candidates
            .into_iter()
            .filter_map(|(session_id, worktree, agent, blocked, idle)| {
                let record = coding::run_registry::get(&self.ctx.data_dir, &session_id);
                walled_run(&session_id, &worktree, agent, &blocked, record.as_ref(), idle)
            })
            .collect();
        self.holds
            .retain(&walled.iter().map(|run| run.session_id.clone()).collect());
        // An agent whose batch is in flight is decided again once it lands.
        let walled: Vec<_> = walled.into_iter().filter(|run| !inflight.busy(run.agent)).collect();
        if walled.is_empty() {
            return;
        }
        // Re-read at beat time: the toggle moves at runtime (remote_admin).
        let settings = coding::Settings::load(&coding::Settings::default_path(&self.ctx.data_dir));
        let plan = lock_or_recover(&self.tracker).plan_beat(walled, settings.auto_rotate_accounts, now_ms);
        for (run, hold) in plan.holds {
            if self.holds.changed(&run.session_id, hold_key(&hold)) {
                log::info!(
                    "account rotation [{}]: walled on {} ({}), holding: {hold:?}",
                    run.session_id,
                    run.account,
                    run.window
                );
            }
        }
        for batch in plan.probes {
            let Some(claim) = coding::account_rotation::InflightProbes::claim_guarded(&self.inflight, &batch) else {
                continue;
            };
            for run in &batch.runs {
                self.holds.clear(&run.session_id);
            }
            self.spawn_probe(batch, claim, doctor.clone(), settings.clone());
        }
    }

    /// ONE forced read of `batch.agent`'s profiles, however many of its
    /// runs are walled, then every run decided off it and switched in turn.
    fn spawn_probe(
        &self,
        batch: coding::account_rotation::ProbeBatch,
        claim: coding::account_rotation::InflightClaim,
        doctor: coding::DoctorReport,
        settings: coding::Settings,
    ) {
        let ctx = Arc::clone(&self.ctx);
        let pickup = Arc::clone(&self.pickup);
        let tracker = Arc::clone(&self.tracker);
        std::thread::spawn(move || {
            // Releases the agent however this thread ends — a panicking
            // probe included, not only the normal path below.
            let _claim = claim;
            let ids: Vec<&str> = batch.runs.iter().map(|run| run.session_id.as_str()).collect();
            log::info!(
                "account rotation [{}]: {} run(s) walled between turns — reading every {} profile's usage once",
                ids.join(", "),
                batch.runs.len(),
                batch.agent.id()
            );
            let now_secs = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|elapsed| elapsed.as_secs())
                .unwrap_or(0);
            let profiles =
                coding::agent_usage::collect_now(batch.agent, &ctx.data_dir, &settings, &doctor, now_secs);
            let now_ms = chrono::Utc::now().timestamp_millis();
            let decisions = {
                let mut tracker = lock_or_recover(&tracker);
                let decisions = tracker.decide_batch(&batch.runs, &profiles, now_ms);
                // The decisions counted: written before they act, so a
                // re-exec in the middle of a switch still remembers them.
                tracker.save(&ctx.data_dir, now_ms);
                decisions
            };
            for (run, decision) in batch.runs.iter().zip(decisions) {
                let switched = act_on_rotation(&pickup, run, decision);
                if !switched {
                    // EXP-1107: a switch refused before it began spends
                    // neither the cap nor the cooldown; the chain retries
                    // after the short spacing.
                    let mut tracker = lock_or_recover(&tracker);
                    let now = chrono::Utc::now().timestamp_millis();
                    tracker.undo_switch(&run.chain_key, now_ms, now);
                    tracker.save(&ctx.data_dir, now);
                }
            }
        });
    }
}

/// Act on one rotation decision for `run`. `false` = a switch refused
/// before it began (an attempt already holds the run, or the agent started a
/// turn): the caller undoes the rotation the tracker counted. A wait, a
/// switch made, or a switch that FAILED (no local record, the run did not
/// stop in time, a launch error) is `true` — like the desktop's, a failure
/// keeps its count and the cooldown spaces the retry, rather than a forced
/// probe of every profile every beat for a run that can never resume.
fn act_on_rotation(
    pickup: &DaemonPickup,
    run: &coding::account_rotation::WalledRun,
    decision: coding::account_rotation::Decision,
) -> bool {
    match decision {
        coding::account_rotation::Decision::Switch {
            target,
            target_name,
            prompt,
        } => {
            log::info!(
                "account rotation [{}]: switching {} -> {target} ({target_name})",
                run.session_id,
                run.account
            );
            match switch_account(pickup, &run.session_id, target, prompt) {
                Ok(()) => true,
                Err(err) if err.is::<SwitchBusy>() => {
                    log::warn!(
                        "account rotation [{}]: switch refused — the agent is working or another attempt holds it; retrying soon",
                        run.session_id
                    );
                    false
                }
                Err(err) => {
                    log::warn!("account rotation [{}]: switch failed: {err:#}", run.session_id);
                    true
                }
            }
        }
        coding::account_rotation::Decision::Wait { until_ms } => {
            log::info!(
                "account rotation [{}]: no profile has headroom — waiting until {until_ms}",
                run.session_id
            );
            true
        }
    }
}

fn lock_or_recover<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    match mutex.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

// ---------------------------------------------------------------------------
// Live sessions
// ---------------------------------------------------------------------------

/// EXP-862 — the logins the runs this daemon hosts are using right now, as
/// `(agent, profile id)` pairs (`agent_profile_remove` and, EXP-1137,
/// `agent_profile_sign_out` refuse to pull one out from under a live run).
/// The live row does not carry the account, the run RECORD does
/// (`runs.json`), so the live ids resolve through the registry — ONE load for
/// the whole pass (`run_registry::all`), never one parse per session. Order
/// follows `session_ids`; unknown ids are skipped, and so is a legacy record
/// that ran on the retired ambient login (it holds no profile). Mirrored by
/// the IDE's `device_sync::live_run_accounts`.
fn live_run_accounts(
    data_dir: &Path,
    session_ids: &[String],
) -> Vec<(coding::CodingAgent, String)> {
    let records = coding::run_registry::all(data_dir);
    session_ids
        .iter()
        .filter_map(|id| records.iter().find(|record| record.session_id == *id))
        .filter_map(|record| Some((record.agent, record.account()?)))
        .collect()
}

/// A mid-turn account switch, refused: typed so the rotation can tell it
/// (retry soon, nothing spent) from a switch that failed.
#[derive(Debug)]
struct SwitchBusy {
    session_id: String,
}

impl std::fmt::Display for SwitchBusy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // The ×4 sentence (`SessionAccountSwitch.REASON_BUSY`), so the log says
        // exactly what the requester's own disabled row says.
        write!(
            f,
            "account switch for {} refused — \
             The agent is working — switching waits for the turn to finish.",
            self.session_id
        )
    }
}

impl std::error::Error for SwitchBusy {}

/// EXP-849/EXP-1005 — move a live, IDLE session onto another account
/// through THE switch ([`coding::pickup::switch_session`], the same one a
/// person's `sessions.send{sessionId, account}` takes): the engine is
/// SUSPENDED (its row idles, its workspace — a scratch dir too — stays) and
/// the SAME row's agent CLI re-spawned natively under `target`, `prompt`
/// (the rotation's note) as the seed turn. The session stays in the
/// pickup's in-flight set throughout.
///
/// `Err(SwitchBusy)` = refused before anything moved (mid-turn, or an attempt
/// already holds the session); any other error is a switch that FAILED.
fn switch_account(
    pickup: &DaemonPickup,
    session_id: &str,
    target: String,
    prompt: String,
) -> anyhow::Result<()> {
    let busy = || SwitchBusy { session_id: session_id.to_string() };
    let Some(_in_flight) = pickup.begin(session_id) else {
        return Err(busy().into());
    };
    let row = pickup
        .store()
        .and_then(|store| session_row(&store, session_id))
        .ok_or_else(|| anyhow::anyhow!("session {session_id} is not synced on this device"))?;
    match coding::pickup::switch_session(&row, &target, Some(&prompt), &[], &pickup.deps()) {
        Ok(coding::pickup::Switched::Moved | coding::pickup::Switched::AlreadyOn) => Ok(()),
        Ok(coding::pickup::Switched::Refused(reason)) => {
            anyhow::bail!("account switch for {session_id} refused: {reason}")
        }
        Err(coding::pickup::SwitchError::Busy) => Err(busy().into()),
        Err(coding::pickup::SwitchError::Failed(reason)) => anyhow::bail!(reason),
    }
}

/// EXP-758 (EXP-478): register a launched run, THEN release its launch gate.
/// The ORDER is the point: a prune builds its `held` set from the
/// live-session list, so one arriving between the launch and this push sees
/// a branch with no unique commits and no live session and removes the
/// worktree under a run that just started. The gate covers exactly that
/// window, and this is where it ends (desktop parity: `ui/src/coding_flow.rs`
/// inserts into `LocalSessions`, then drops).
fn register_session(sessions: &Sessions, live: LiveSession) {
    let session = Arc::clone(&live.session);
    lock_sessions(sessions).push(live);
    session.release_launch_hold();
}

// ---------------------------------------------------------------------------
// EXP-481: the device-state worker — one background thread that converges
// the server-authoritative launch defaults, executes pulled worktree
// commands, and reports the worktree inventory. Serialized by design (one
// prune at a time); the 1s daemon loop only ever SENDS to it.
// ---------------------------------------------------------------------------

/// Work the daemon loop hands the device worker.
enum DeviceWork {
    /// The server copy of the launch defaults was observed (register
    /// response / heartbeat stamp mismatch) — reconcile against it.
    ServerDefaults {
        defaults: Option<serde_json::Value>,
        stamp: Option<String>,
    },
    /// No fresh server copy this cycle — still detect + push local edits
    /// (hand-edited settings.json, marker left dirty by an offline push).
    ReconcileLocal,
    /// Pending commands pulled off a heartbeat. Redelivery until completed
    /// is the server's idempotency model — repeats are expected and safe.
    Commands(Vec<api::devices::PendingCommand>),
    /// Re-scan the worktrees and report when the fingerprint moved.
    ReportWorktrees,
    /// EXP-484: collect the agent accounts + usage windows for the NEXT
    /// heartbeat. Runs here because `collect_if_due` blocks (keychain read,
    /// an HTTP fetch, a codex app-server spawn — up to ~10s) and the daemon
    /// loop must stay at 1Hz. `report` is the loop's last doctor pass: the
    /// collector reads it for which binaries exist and who is signed in
    /// (boxed — it dwarfs every other variant). EXP-1158: `then_beat` raises
    /// the loop's check-in once the collection landed (a session change).
    CollectAgentStatus {
        report: Box<coding::DoctorReport>,
        then_beat: bool,
    },
}

#[allow(clippy::too_many_arguments)]
fn spawn_device_worker(
    ctx: Arc<Ctx>,
    sessions: Sessions,
    device_id: String,
    agent_status: Arc<Mutex<Option<coding::AgentStatusPayload>>>,
    doctor_soon: Arc<AtomicBool>,
    update_now: Arc<AtomicBool>,
    // EXP-484: `agent_login` runs on its own thread (a PTY that lives for
    // minutes must not block this worker) — the set is what makes a
    // REDELIVERED command id a no-op instead of a second sign-in. An
    // `agent_update` (minutes of download) claims the same set, and the
    // loop's self-update restart holds while it is non-empty.
    logins_inflight: Arc<Mutex<HashSet<String>>>,
    // EXP-1158: the loop's beat-now flag, raised after a `then_beat`
    // collection.
    check_in: Arc<AtomicBool>,
) -> flume::Sender<DeviceWork> {
    let (tx, rx) = flume::unbounded::<DeviceWork>();
    std::thread::spawn(move || {
        let mut last_inventory_fp: Option<u64> = None;
        // EXP-765: where a live login's requester drops the authorization
        // code claude's browser page showed (one slot per agent).
        let login_codes: crate::agent_login_host::CodeInbox =
            Arc::new(Mutex::new(std::collections::HashMap::new()));
        while let Ok(work) = rx.recv() {
            match work {
                DeviceWork::ServerDefaults { defaults, stamp } => {
                    reconcile_defaults(&ctx, &device_id, defaults.as_ref(), stamp.as_deref());
                }
                DeviceWork::ReconcileLocal => {
                    let settings_path = coding::Settings::default_path(&ctx.data_dir);
                    let marker = coding::read_marker(&settings_path, &device_id);
                    // Without a fresh server observation, pretend the server
                    // is unchanged — reconcile() then pushes only for local
                    // edits (dirty marker / fingerprint drift) or the
                    // first-ever seed.
                    reconcile_defaults_with(
                        &ctx,
                        &device_id,
                        None,
                        marker.synced_at.clone().as_deref(),
                        marker.synced_at.is_some(),
                    );
                }
                DeviceWork::Commands(commands) => {
                    for command in commands {
                        run_device_command(
                            &ctx,
                            &sessions,
                            &command,
                            &logins_inflight,
                            &login_codes,
                            &doctor_soon,
                            &CommandSlots {
                                agent_status: &agent_status,
                                update_now: &update_now,
                            },
                        );
                    }
                    report_worktrees(&ctx, &sessions, &device_id, &mut last_inventory_fp);
                }
                DeviceWork::ReportWorktrees => {
                    report_worktrees(&ctx, &sessions, &device_id, &mut last_inventory_fp);
                }
                DeviceWork::CollectAgentStatus { report, then_beat } => {
                    let settings =
                        coding::Settings::load(&coding::Settings::default_path(&ctx.data_dir));
                    let payload = coding::collect_if_due(
                        &ctx.data_dir,
                        &settings,
                        &report,
                        coding::run_registry::now_secs(),
                    );
                    if let Ok(mut slot) = agent_status.lock() {
                        *slot = Some(payload);
                    }
                    if then_beat {
                        check_in.store(true, Ordering::SeqCst);
                    }
                }
            }
        }
    });
    tx
}

/// EXP-792: the worker-owned slots a command may write (the forced usage
/// refresh fills the status slot).
struct CommandSlots<'a> {
    agent_status: &'a Arc<Mutex<Option<coding::AgentStatusPayload>>>,
    /// FEED-36: `update_now` sets it after ending the live sessions.
    update_now: &'a Arc<AtomicBool>,
}

/// Reconcile against an OBSERVED server copy.
fn reconcile_defaults(
    ctx: &Ctx,
    device_id: &str,
    server_defaults: Option<&serde_json::Value>,
    server_stamp: Option<&str>,
) {
    reconcile_defaults_with(
        ctx,
        device_id,
        server_defaults,
        server_stamp,
        server_defaults.is_some(),
    );
}

fn reconcile_defaults_with(
    ctx: &Ctx,
    device_id: &str,
    server_defaults: Option<&serde_json::Value>,
    server_stamp: Option<&str>,
    server_has_defaults: bool,
) {
    let settings_path = coding::Settings::default_path(&ctx.data_dir);
    let settings = coding::Settings::load(&settings_path);
    let fingerprint = coding::defaults_fingerprint(&settings);
    let marker = coding::read_marker(&settings_path, device_id);
    match coding::reconcile(&marker, &fingerprint, server_stamp, server_has_defaults) {
        coding::ReconcileAction::Noop => {}
        coding::ReconcileAction::ApplyServer => match server_defaults {
            Some(value) => apply_server_defaults(ctx, device_id, value, server_stamp),
            // The stamp moved but no copy rode along (row recreated with a
            // NULL column) — seed it back with a CAS-guarded push.
            None => push_local_defaults(ctx, device_id, &settings, &marker),
        },
        coding::ReconcileAction::PushLocal => {
            push_local_defaults(ctx, device_id, &settings, &marker)
        }
    }
}

fn apply_server_defaults(
    ctx: &Ctx,
    device_id: &str,
    value: &serde_json::Value,
    stamp: Option<&str>,
) {
    let settings_path = coding::Settings::default_path(&ctx.data_dir);
    let patch: coding::DefaultsPatch = match serde_json::from_value(value.clone()) {
        Ok(patch) => patch,
        Err(err) => {
            log::warn!("launch defaults: unparsable server copy ignored: {err}");
            return;
        }
    };
    let mut settings = coding::Settings::load(&settings_path);
    let was_on = settings.computer_use;
    let changed = coding::apply_defaults_patch(&mut settings, &patch);
    if changed {
        if let Err(err) = settings.save(&settings_path) {
            log::warn!("launch defaults: save failed: {err}");
            return;
        }
        log::info!("launch defaults: applied the server copy");
        // EXP-1196: the switch flipped from another client: on = ask this
        // machine's permissions now, not in the middle of a run; off = end
        // every grant and give the person their input back.
        if settings.computer_use && !was_on {
            coding::computer::prepare_in_background();
        } else if !settings.computer_use && was_on {
            std::thread::spawn(coding::computer::shutdown);
        }
    }
    // Clamped/invalid fields are deliberately NOT pushed back (ping-pong);
    // recording the stamp stops a re-apply loop either way.
    let marker = coding::SyncMarker {
        synced_at: stamp.map(str::to_string),
        dirty: false,
        hash: Some(coding::defaults_fingerprint(&settings)),
    };
    if let Err(err) = coding::write_marker(&settings_path, device_id, &marker) {
        log::debug!("launch defaults: marker write failed: {err}");
    }
}

fn push_local_defaults(
    ctx: &Ctx,
    device_id: &str,
    settings: &coding::Settings,
    marker: &coding::SyncMarker,
) {
    let settings_path = coding::Settings::default_path(&ctx.data_dir);
    let wire = serde_json::to_value(coding::defaults_wire(settings))
        .expect("defaults serialize cannot fail");
    let expected = api::devices::ExpectedStamp::Expect(marker.synced_at.as_deref());
    match api::devices::set_launch_defaults(&ctx.trpc, device_id, &wire, expected) {
        Ok(result) if result.conflict => {
            // Server wins the offline-concurrent race — adopt its copy.
            log::info!("launch defaults: push conflicted — adopting the server copy");
            if let Some(value) = result.launch_defaults.as_ref() {
                apply_server_defaults(
                    ctx,
                    device_id,
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
            if let Err(err) = coding::write_marker(&settings_path, device_id, &marker) {
                log::debug!("launch defaults: marker write failed: {err}");
            }
        }
        Err(err) => {
            // Offline / transport error: queue the retry (next heartbeat's
            // ReconcileLocal picks the dirty flag up).
            log::debug!("launch defaults: push failed ({err}) — queued");
            let marker = coding::SyncMarker {
                dirty: true,
                hash: marker.hash.clone(),
                synced_at: marker.synced_at.clone(),
            };
            let _ = coding::write_marker(&settings_path, device_id, &marker);
        }
    }
}

/// Execute one pulled command and report its outcome. `ok: false` from
/// completeCommand means a redelivered duplicate raced us — fine.
fn run_device_command(
    ctx: &Arc<Ctx>,
    sessions: &Sessions,
    command: &api::devices::PendingCommand,
    logins_inflight: &Arc<Mutex<HashSet<String>>>,
    login_codes: &crate::agent_login_host::CodeInbox,
    doctor_soon: &Arc<AtomicBool>,
    slots: &CommandSlots<'_>,
) {
    let settings = coding::Settings::load(&coding::Settings::default_path(&ctx.data_dir));
    let (ok, message) = match command.kind.as_str() {
        // EXP-484: a sign-in on this machine, requested from anywhere. The
        // PTY lives for minutes, so the host owns its own thread and its own
        // completion — this arm never falls through to the one below.
        "agent_login" => {
            crate::agent_login_host::run(
                ctx,
                settings,
                command.clone(),
                Arc::clone(logins_inflight),
                Arc::clone(login_codes),
                Arc::clone(doctor_soon),
            );
            return;
        }
        // EXP-765: the code for the login above — typed into its PTY and
        // completed on the spot, by the host that owns the slot.
        "agent_login_code" => {
            crate::agent_login_host::enter_code(ctx, command, login_codes);
            return;
        }
        // EXP-792: force one agent's usage re-read past the shared TTL
        // (never past the 429 floor — a hot refusal names when).
        "agent_usage_refresh" => {
            let agent = command.payload["agent"].as_str().unwrap_or_default();
            let profile = command.payload["profileId"].as_str().unwrap_or_default();
            match coding::CodingAgent::parse(agent) {
                None => (false, "Malformed command payload.".to_string()),
                Some(agent) => {
                    let report = coding::run_doctor(&settings, &ctx.data_dir);
                    // EXP-808: every account PROFILE refreshes, not just the
                    // device's default one.
                    match coding::force_collect(
                        &ctx.data_dir,
                        &settings,
                        &report,
                        agent,
                        profile,
                        coding::run_registry::now_secs(),
                    ) {
                        Ok(payload) => {
                            if let Ok(mut slot) = slots.agent_status.lock() {
                                *slot = Some(payload);
                            }
                            (true, format!("Refreshed {} usage.", agent.id()))
                        }
                        Err(until) => (
                            false,
                            format!(
                                "{} is rate-limited; try again after {}.",
                                agent.id(),
                                coding::agent_accounts::iso_from_unix_secs(until as i64)
                                    .unwrap_or_else(|| until.to_string())
                            ),
                        ),
                    }
                }
            }
        }
        // EXP-862 — "remove account": delete this machine's copy of a login
        // (the profile dir, credentials included, and its index row). The
        // ACCOUNT is untouched — never `codex logout`, which revokes it
        // server-wide — and nothing leaves the machine. An account a live run
        // here is using is refused.
        //
        // EXP-1137 — "sign out": the same guard, and the login's credential
        // goes the agent's own way while its row stays.
        //
        // Both raise `doctor_soon`: the login's row in the cached report is
        // stale the moment it signs out, and the advertisement (runnable
        // agents) follows the fresh check on the next loop turn.
        kind @ ("agent_profile_remove" | "agent_profile_sign_out") => {
            let agent = command.payload["agent"].as_str().unwrap_or_default();
            let profile = command.payload["profileId"].as_str().unwrap_or_default();
            match coding::CodingAgent::parse(agent) {
                None => (false, "Malformed command payload.".to_string()),
                Some(agent) => {
                    let report = coding::run_doctor(&settings, &ctx.data_dir);
                    // The accounts the runs THIS daemon hosts are on: the
                    // live row does not carry one, its run record does. The
                    // ids are copied out under the sessions lock and the
                    // registry read AFTER it is released — one `runs.json`
                    // load, never a file parse per session with the mutex
                    // held.
                    let live_ids: Vec<String> = lock_sessions(sessions)
                        .iter()
                        .filter(|live| !live.session.is_done())
                        .map(|live| live.session.session_id.clone())
                        .collect();
                    let live_accounts = live_run_accounts(&ctx.data_dir, &live_ids);
                    let now = coding::run_registry::now_secs();
                    let (outcome, done) = if kind == "agent_profile_remove" {
                        (
                            coding::agent_usage::remove_profile(
                                &ctx.data_dir,
                                &settings,
                                &report,
                                agent,
                                profile,
                                &live_accounts,
                                now,
                            ),
                            format!("The {} account was removed from this machine.", agent.id()),
                        )
                    } else {
                        (
                            coding::agent_usage::sign_out_profile(
                                &ctx.data_dir,
                                &settings,
                                &report,
                                agent,
                                profile,
                                &live_accounts,
                                now,
                            ),
                            format!("The {} account was signed out on this machine.", agent.id()),
                        )
                    };
                    match outcome {
                        Ok((payload, _)) => {
                            if let Ok(mut slot) = slots.agent_status.lock() {
                                *slot = Some(payload);
                            }
                            doctor_soon.store(true, Ordering::SeqCst);
                            (true, done)
                        }
                        Err(error) => (false, error),
                    }
                }
            }
        }
        // FEED-36: the web's "Update now" — stop every live session's process
        // (the owner confirmed it; EXP-1292: suspended, so each row idles and
        // the next message resumes it) and let the loop apply the pending
        // update as soon as they close.
        "update_now" => {
            let live: Vec<(String, String)> = lock_sessions(sessions)
                .iter()
                .filter(|live| !live.session.is_done())
                .map(|live| (live.session.session_id.clone(), live.branch.clone()))
                .collect();
            for (session_id, branch) in &live {
                log::info!("update now (web): suspending session {session_id} ({branch})");
            }
            for live_session in lock_sessions(sessions).iter() {
                live_session.session.suspend();
            }
            slots.update_now.store(true, Ordering::SeqCst);
            if live.is_empty() {
                (true, "Restarting on the new version.".to_string())
            } else {
                (
                    true,
                    format!(
                        "Pausing {} live session(s); the daemon restarts on the new version once they close.",
                        live.len()
                    ),
                )
            }
        }
        // The agent CLI's own self-updater, requested from the device
        // settings dialog (the FEED-36 twin for claude/codex). Minutes of
        // download, so it owns its own thread and its own completion like a
        // sign-in does — and the same claim set, since the pending row rides
        // every heartbeat until `completeCommand` lands and a second pull
        // must not start a second updater. Live sessions keep running (the
        // binary they hold stays mapped); the doctor re-probes right after
        // so the next beat's `agent_accounts.<agent>.version` moves.
        coding::AGENT_UPDATE_COMMAND => {
            let agent = command.payload["agent"].as_str().unwrap_or_default();
            let Some(agent) = coding::CodingAgent::parse(agent) else {
                let message = "Malformed command payload.".to_string();
                if let Err(err) =
                    api::devices::complete_command(&ctx.trpc, &command.id, false, Some(&message))
                {
                    log::debug!("completeCommand failed (redelivery will retry): {err}");
                }
                return;
            };
            {
                let Ok(mut guard) = logins_inflight.lock() else {
                    return;
                };
                if !guard.insert(command.id.clone()) {
                    log::debug!(
                        "agent_update {} already in flight — ignoring the redelivery",
                        command.id
                    );
                    return;
                }
            }
            let ctx = Arc::clone(ctx);
            let command_id = command.id.clone();
            let claimed = Arc::clone(logins_inflight);
            let doctor_soon = Arc::clone(doctor_soon);
            let spawned = std::thread::Builder::new()
                .name("exp-agent-update".to_string())
                .spawn(move || {
                    log::info!("agent update (web): running {} update", agent.id());
                    let (ok, message) = match coding::update_agent(&settings, &ctx.data_dir, agent) {
                        Ok(outcome) => (true, outcome.message()),
                        Err(error) => (false, error),
                    };
                    log::info!("agent update ({}): {message}", agent.id());
                    if let Err(err) =
                        api::devices::complete_command(&ctx.trpc, &command_id, ok, Some(&message))
                    {
                        log::debug!("completeCommand failed (redelivery will retry): {err}");
                    }
                    if let Ok(mut guard) = claimed.lock() {
                        guard.remove(&command_id);
                    }
                    doctor_soon.store(true, Ordering::SeqCst);
                });
            if spawned.is_err() {
                if let Ok(mut guard) = logins_inflight.lock() {
                    guard.remove(&command.id);
                }
                log::info!("agent update: could not spawn the updater thread");
            }
            return;
        }
        other => {
            log::info!("device command {other:?} unsupported — reported back");
            (false, "This machine's app doesn't support that command yet.".to_string())
        }
    };
    if let Err(err) = api::devices::complete_command(&ctx.trpc, &command.id, ok, Some(&message)) {
        log::debug!("completeCommand failed (redelivery will retry): {err}");
    }
}

/// EXP-1353: offer every clone under `repos_root` to the shared cargo target
/// GC (`coding::cargo_target_gc`, throttled per clone). Returns the clones
/// actually walked.
fn sweep_cargo_targets(repos_root: &Path) -> Vec<String> {
    let mut walked = Vec::new();
    for clone in coding::scan_clones(repos_root) {
        let Some(report) = coding::cargo_target_gc::sweep_if_due(&clone.path) else {
            continue;
        };
        if report.bytes_freed > 0 || !report.skipped_locked.is_empty() {
            log::info!(
                "cargo-target gc {}: freed {} of {} bytes, skipped locked {:?}",
                clone.full_name,
                report.bytes_freed,
                report.bytes_before,
                report.skipped_locked,
            );
        }
        walked.push(clone.full_name);
    }
    walked
}

/// NEW-5: the nightly stale-worktree sweep over every clone under
/// `repos_root` ([`coding::sweep_stale_worktrees_if_due`]: once a day per
/// clone), logging each report and naming parked leftover branches in their
/// sessions' conversations. Returns the clones swept this call.
fn sweep_stale_worktrees(
    repos_root: &Path,
    data_dir: &Path,
    live: coding::LiveHolders,
    trpc: &api::TrpcClient,
) -> Vec<String> {
    let policy = coding::SweepPolicy { live, ..coding::SweepPolicy::default() };
    let mut swept = Vec::new();
    for clone in coding::scan_clones(repos_root) {
        let Some(report) =
            coding::sweep_stale_worktrees_if_due(&clone.path, data_dir, std::time::SystemTime::now(), &policy)
        else {
            continue;
        };
        coding::settle_sweep_report(Some(trpc), &clone.full_name, &report);
        swept.push(clone.full_name);
    }
    swept
}

/// Scan + report the worktree inventory when its fingerprint moved.
fn report_worktrees(
    ctx: &Ctx,
    sessions: &Sessions,
    device_id: &str,
    last_fp: &mut Option<u64>,
) {
    let settings = coding::Settings::load(&coding::Settings::default_path(&ctx.data_dir));
    let inventory = coding::scan_inventory(&settings.repos_root_path());
    let busy: std::collections::HashSet<String> = lock_sessions(sessions)
        .iter()
        .filter(|live| !live.session.is_done())
        .map(|live| live.branch.clone())
        .collect();
    let fp = coding::inventory_fingerprint(&inventory, &busy);
    if *last_fp == Some(fp) {
        return;
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
            busy: busy.contains(&entry.branch),
        })
        .collect();
    match api::devices::report_worktrees(&ctx.trpc, device_id, &rows) {
        Ok(()) => *last_fp = Some(fp),
        // Older server / offline: retried on the next trigger (fp unchanged).
        Err(err) => log::debug!("reportWorktrees failed: {err}"),
    }
}

// ---------------------------------------------------------------------------
// EXP-1292: the pickup host — this daemon's own Electric pipeline (a shape
// subset in `sync-cli.sqlite`), ONE delta-drain thread, and ONE serialized
// worker. Each pass runs the session pickup (the database is the bus:
// `queued` rows naming this device launch, person messages deliver, a row
// gone `ended` kills — `coding::pickup`). NEW-2: agent triggers are fired
// by the SERVER (a message into the agent's conversation session), never
// here. Sessions it spawns join the normal
// `Sessions` vec, so heartbeat/parked-update/quit-sweep cover them like any
// other run.
// ---------------------------------------------------------------------------

/// What the daemon syncs: the session tree and its conversation (`sessions`,
/// `messages`, `session_issues` — the pickup's input) and the rows the
/// branch names read (`issues`, `boards`). Deliberately NOT the desktop's
/// whole set — a
/// headless daemon has no views to hydrate. A name the sync crate has no
/// spec for is skipped by the pipeline (the v2 specs land with #EXP-1294).
const DAEMON_SHAPES: &[&str] = &[
    "sessions",
    "messages",
    // NEW-8: a message's session is its conversation's.
    "conversations",
    "session_issues",
    "issues",
    "boards",
];

/// The shapes a pickup pass reads. It runs only once BOTH reached
/// `up-to-date` in this process: `coding::pickup::plan` kills a live session
/// whose row is missing, so a cold store must never be planned over.
/// NEW-8: the messages wait on their conversations too (a message whose
/// conversation has not synced is held back).
const PICKUP_SHAPES: &[&str] = &["sessions", "messages", "conversations"];

/// The worker's self-tick. Deltas drive the pickup (it acts within a second
/// of the row landing); this beat is the catch-up pass after a
/// sleep/offline stretch where no delta ever arrives.
const DAEMON_TICK: Duration = Duration::from_secs(30);

/// How long a failed launch/resume attempt waits before the pass retries it
/// (a claim that failed over the network leaves the row `queued`).
const PICKUP_RETRY: Duration = Duration::from_secs(30);

/// NEW-5: how long the heartbeat counts the sessions an earlier process
/// left behind as live while the startup re-claim waits for the shapes (a
/// sync that never comes up must not hide a dead turn forever).
const RECLAIM_GRACE: Duration = Duration::from_secs(5 * 60);

/// EXP-1334: how long a switch / reconfigure that a turn in flight refused
/// waits before the pass plans it again (the turn's own rows nudge the
/// pass; a full [`PICKUP_RETRY`] would hold the session's deliveries,
/// answers and kills with it).
const SWITCH_BUSY_RETRY: Duration = Duration::from_secs(2);

/// Open the daemon's shape store and start its pipeline. `None` (logged, not
/// fatal) leaves the pickup dormant while every other daemon
/// duty — heartbeat, worktree commands, history serving — keeps running.
fn start_daemon_sync(ctx: &Ctx, gated: &Arc<AtomicBool>) -> Option<Arc<sync::SyncManager>> {
    // NEVER `sync-v2.sqlite`: that file belongs to the desktop GUI, and two
    // pipelines writing one store would fight over shape offsets.
    let db_path = ctx
        .data_dir
        .join("accounts")
        .join(&ctx.account.id)
        .join("sync-cli.sqlite");
    if let Some(parent) = db_path.parent() {
        if let Err(err) = std::fs::create_dir_all(parent) {
            log::warn!("daemon sync: store dir ({err}) — sessions disabled");
            return None;
        }
    }
    // EXP-641: the pipeline's 426 hook (the desktop routes it to the blocking
    // "Update required" view; the daemon has nobody to show that to, so it
    // self-updates instead — see the loop's gated trigger).
    let on_upgrade_required: sync::UpgradeRequiredFn = {
        let gated = Arc::clone(gated);
        Arc::new(move || {
            gated.store(true, Ordering::SeqCst);
        })
    };
    let manager = Arc::new(sync::SyncManager::new().on_upgrade_required(on_upgrade_required));
    let config = sync::AccountSyncConfig {
        account_id: ctx.account.id.clone(),
        base_url: ctx.account.instance_url.clone(),
        db_path,
        // Call-time token access (§5.7) — never captured once, so a refresh
        // mid-run is picked up by the next poll.
        token: ctx.auth.token_provider_fn(&ctx.account.id),
        shapes: Some(DAEMON_SHAPES),
    };
    match manager.start_account(config) {
        Ok(_) => {
            // EXP-533: a suspended (or hibernated) host leaves every shape
            // thread parked in a read on a connection that died with it —
            // the daemon has the same bug the GUI had, and the same fix.
            sync::spawn_wake_watchdog(&manager);
            Some(manager)
        }
        Err(err) => {
            log::warn!("daemon sync: store failed to open ({err}) — sessions disabled");
            None
        }
    }
}

/// Why the worker woke up. Every variant runs the same full pass.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DaemonWork {
    /// A `sessions` / `messages` / `session_issues` batch landed (a queued
    /// start, a message), or a pickup attempt
    /// settled and the pass should look again.
    Pickup,
    /// The 1s loop's [`DAEMON_TICK`] beat.
    Tick,
}

/// The shapes that reached `up-to-date` in this process (the drain records
/// them; the pickup pass waits for [`PICKUP_SHAPES`]).
#[derive(Default)]
struct SyncedShapes(Mutex<HashSet<&'static str>>);

impl SyncedShapes {
    /// Record a batch's `up-to-date`; `true` = the shape just reached it.
    fn mark(&self, shape: &'static str) -> bool {
        lock_or_recover(&self.0).insert(shape)
    }

    fn ready(&self, shapes: &[&str]) -> bool {
        let synced = lock_or_recover(&self.0);
        shapes.iter().all(|shape| synced.contains(shape))
    }
}

/// The work a delta wakes, `None` = nothing the worker reads.
fn delta_work(shape: &str) -> Option<DaemonWork> {
    match shape {
        "sessions" | "messages" | "conversations" | "session_issues" => Some(DaemonWork::Pickup),
        _ => None,
    }
}

/// The single `deltas()` consumer (flume is MPMC — cloned receivers STEAL,
/// so exactly one place may drain). Applied batches of the shapes the worker
/// reads nudge it; everything else is ignored.
fn spawn_delta_drain(
    manager: &Arc<sync::SyncManager>,
    synced: Arc<SyncedShapes>,
    worker: flume::Sender<DaemonWork>,
) {
    let deltas = manager.deltas();
    let spawned = std::thread::Builder::new()
        .name("exp-daemon-sync".to_string())
        .spawn(move || {
            while let Ok(delta) = deltas.recv() {
                let sync::ShapeDelta::Applied {
                    shape,
                    keys,
                    full_replace,
                    up_to_date,
                    ..
                } = delta
                else {
                    continue;
                };
                let first_sync = up_to_date && synced.mark(shape);
                let Some(work) = delta_work(shape) else {
                    continue;
                };
                // A pure `up-to-date` heartbeat changed nothing — unless it is
                // the shape's FIRST, which may unlock the pickup pass.
                if keys.is_empty() && !full_replace && !first_sync {
                    continue;
                }
                // A closed channel is the daemon shutting down.
                if worker.send(work).is_err() {
                    break;
                }
            }
        });
    if let Err(err) = spawned {
        log::warn!("daemon sync: delta drain failed to spawn: {err}");
    }
}

/// Removes its session from the in-flight set on drop — including a panic
/// in the attempt, so a crashed launch never wedges its row.
struct InFlight {
    set: Arc<Mutex<HashSet<String>>>,
    session_id: String,
}

impl Drop for InFlight {
    fn drop(&mut self) {
        lock_or_recover(&self.set).remove(&self.session_id);
    }
}

/// EXP-1292: what every pickup step needs — shared by the worker (the pass)
/// and the rotation beat (an account switch relaunches a run).
struct DaemonPickup {
    ctx: Arc<Ctx>,
    runtime: Option<Arc<steer::SteerRuntime>>,
    personal_key: Option<String>,
    sessions: Sessions,
    live: Arc<coding::pickup::LiveRegistry>,
    sync: Option<Arc<sync::SyncManager>>,
    device_id: String,
    /// Sessions an attempt works on right now (a launch, a resume, an
    /// account switch): the pass hands them nothing until it settles.
    in_flight: Arc<Mutex<HashSet<String>>>,
    /// A failed attempt's retry floor ([`PICKUP_RETRY`]).
    backoff: Mutex<HashMap<String, Instant>>,
    /// NEW-5: the sessions an earlier process left behind, awaiting the
    /// startup re-claim ([`DaemonHost::reconcile_rows`]); the heartbeat
    /// counts them live until then (at most until `reclaim_until`).
    reclaiming: Mutex<HashSet<String>>,
    reclaim_until: Instant,
}

impl DaemonPickup {
    fn store(&self) -> Option<Arc<sync::store::ShapeStore>> {
        self.sync.as_ref()?.store(&self.ctx.account.id)
    }

    /// Mark a session in flight; `None` = an attempt already holds it.
    fn begin(&self, session_id: &str) -> Option<InFlight> {
        lock_or_recover(&self.in_flight)
            .insert(session_id.to_string())
            .then(|| InFlight {
                set: Arc::clone(&self.in_flight),
                session_id: session_id.to_string(),
            })
    }

    fn back_off(&self, session_id: &str) {
        self.back_off_for(session_id, PICKUP_RETRY);
    }

    /// [`Self::back_off`] for `wait`.
    fn back_off_for(&self, session_id: &str, wait: Duration) {
        lock_or_recover(&self.backoff).insert(session_id.to_string(), Instant::now() + wait);
    }

    /// NEW-5: the heartbeat's `liveSessionIds` — every session a process of
    /// this machine runs: this daemon's runs and engines, its attempts in
    /// flight, the rows awaiting the startup re-claim, and a live sibling
    /// process's (a foreground `code`, EXP-229). `None` past the cap.
    fn heartbeat_live_ids(&self) -> Option<Vec<String>> {
        let own: Vec<String> = lock_sessions(&self.sessions)
            .iter()
            .map(|live| live.session.session_id.clone())
            .collect();
        let in_flight: Vec<String> = lock_or_recover(&self.in_flight).iter().cloned().collect();
        let reclaiming: Vec<String> = if Instant::now() < self.reclaim_until {
            lock_or_recover(&self.reclaiming).iter().cloned().collect()
        } else {
            Vec::new()
        };
        live_session_ids([
            own,
            self.live.session_ids(),
            in_flight,
            reclaiming,
            registry::live_ids(&self.ctx.data_dir),
        ])
    }

    /// EXP-1374: the heartbeat's `sessionActivity` — every engine this
    /// daemon hosts, with the wall-clock time of its last activity.
    fn heartbeat_activity(&self) -> Vec<api::devices::SessionActivity> {
        lock_sessions(&self.sessions)
            .iter()
            .map(|live| api::devices::SessionActivity {
                session_id: live.session.session_id.clone(),
                at: coding::agent_accounts::iso_from_system_time(live.session.last_activity_at()),
            })
            .collect()
    }

    /// The pass's `in_flight` set: attempts running and attempts backing off.
    fn excluded(&self) -> HashSet<String> {
        let now = Instant::now();
        let mut excluded = lock_or_recover(&self.in_flight).clone();
        {
            let mut backoff = lock_or_recover(&self.backoff);
            backoff.retain(|_, until| *until > now);
            excluded.extend(backoff.keys().cloned());
        }
        excluded
    }

    /// The pickup's collaborators for one pass — fresh settings each time
    /// (they move at runtime through `remote_admin`).
    fn deps(&self) -> coding::pickup::PickupDeps {
        let settings = coding::Settings::load(&coding::Settings::default_path(&self.ctx.data_dir));
        // The host defaults under every row's pickers; the row decides plan
        // mode (F7: an unattended start never parks unless it asked to).
        let mut options = LaunchOptions::defaults_for(&settings, settings.default_agent);
        options.plan_mode = false;
        let account_id = self.ctx.account.id.clone();
        let own_user = self.ctx.account.user_id.clone();
        let identifiers_sync = self.sync.clone();
        let identifiers_account = account_id.clone();
        let foreign_sync = self.sync.clone();
        let sessions = Arc::clone(&self.sessions);
        launch::pickup_deps(launch::PickupHost {
            ctx: Arc::clone(&self.ctx),
            runtime: self.runtime.clone(),
            personal_key: self.personal_key.clone(),
            rotation_host: true,
            settings,
            options,
            live: Arc::clone(&self.live),
            issue_identifiers: Arc::new(move |root_id: &str| {
                identifiers_sync
                    .as_ref()
                    .and_then(|sync| sync.store(&identifiers_account))
                    .map(|store| root_issue_identifiers(&store, root_id))
                    .unwrap_or_default()
            }),
            // EXP-444: a teammate's start on this shared machine.
            foreign_host: Arc::new(move |session_id: &str| {
                foreign_sync
                    .as_ref()
                    .and_then(|sync| sync.store(&account_id))
                    .and_then(|store| session_row(&store, session_id))
                    .and_then(|row| row.user_id)
                    .is_some_and(|starter| starter != own_user)
            }),
            on_launched: Arc::new(move |session, cleanup| {
                register_session(
                    &sessions,
                    LiveSession {
                        branch: session.branch.clone(),
                        cleanup,
                        session,
                        idle_since: None,
                    },
                );
            }),
        })
    }
}

/// Whether a foreground `exponential code`/`run` on this machine is between
/// its `sessions.send` and its claim ([`launch::FOREGROUND_START_LOCK`]) —
/// the row it queued names THIS device, so the daemon keeps its hands off
/// every launch until then.
fn foreground_start_held(data_dir: &Path) -> bool {
    matches!(
        coding::pickup::ClaimLock::try_acquire(data_dir, launch::FOREGROUND_START_LOCK),
        Ok(None)
    )
}

/// Everything the worker thread owns.
struct DaemonHost {
    pickup: Arc<DaemonPickup>,
    synced: Arc<SyncedShapes>,
    /// The worker's own queue: a settled attempt asks for the next pass.
    nudge: flume::Sender<DaemonWork>,
    /// The startup reconcile of the store's rows ran (once per process).
    reconciled: bool,
}

/// The `spawn_device_worker` idiom: unbounded flume + ONE thread, so two
/// passes never plan concurrently.
fn spawn_daemon_worker(
    pickup: Arc<DaemonPickup>,
    synced: Arc<SyncedShapes>,
) -> flume::Sender<DaemonWork> {
    let (tx, rx) = flume::unbounded::<DaemonWork>();
    let mut host = DaemonHost {
        pickup,
        synced,
        nudge: tx.clone(),
        reconciled: false,
    };
    std::thread::Builder::new()
        .name("exp-daemon-worker".to_string())
        .spawn(move || {
            while rx.recv().is_ok() {
                // Coalesce a burst (a refetch is thousands of rows in a
                // handful of batches) into ONE pass.
                while rx.try_recv().is_ok() {}
                host.beat();
            }
        })
        .expect("spawn the daemon worker");
    tx
}

/// The session a pickup action is about.
/// EXP-1334: what a [`coding::pickup::PickupAction::Switch`] is, for the
/// log: an empty `account` is a reconfigure.
fn switch_kind(account: &str) -> &'static str {
    if account.is_empty() {
        "reconfigure"
    } else {
        "account switch"
    }
}

fn action_session_id(action: &coding::pickup::PickupAction) -> &str {
    action.session_id()
}

/// Did a wake (a Resume, or a delivery that had to re-spawn) hand anything
/// to an engine? Nothing handed = not settled: the caller backs the row off
/// ([`PICKUP_RETRY`]) — a claim a sibling process holds included, since that
/// process may be on its way out (a daemon being replaced) and its claim
/// stale by the retry.
fn woke(result: Result<Vec<String>, coding::pickup::WakeError>) -> bool {
    match result {
        Ok(handed) => !handed.is_empty(),
        Err(coding::pickup::WakeError::Contended) => {
            log::info!(
                "pickup: the session is held by another process here — retrying in {}s",
                PICKUP_RETRY.as_secs()
            );
            false
        }
        Err(err) => {
            log::warn!("pickup: {err}");
            false
        }
    }
}

impl DaemonHost {
    /// One pass. Every failure mode degrades to "do nothing this beat" — the
    /// next nudge or tick retries from fresh state.
    fn beat(&mut self) {
        let Some(store) = self.pickup.store() else {
            return;
        };
        if self.synced.ready(PICKUP_SHAPES) {
            let rows = read_shape_rows::<SessionRow>(&store, "sessions");
            let messages = read_keyed_messages(&store);
            self.pickup_pass(&rows, &messages);
        }
    }

    /// The session pickup: plan over the shapes, then act. Launches and
    /// resumes block on the network and git (a first clone takes minutes),
    /// so each runs on its own thread with its row held in flight; the rest
    /// only touch a live engine and run right here.
    fn pickup_pass(&mut self, rows: &[SessionRow], messages: &[MessageRow]) {
        let live = &self.pickup.live;
        for session_id in live.prune() {
            log::debug!("pickup: session {session_id}'s engine is gone");
        }
        live.settle_handed(messages);
        live.settle_options(rows);
        if !self.reconciled {
            self.reconciled = true;
            self.reconcile_rows(rows, messages);
        }
        let excluded = self.pickup.excluded();
        let views = live.live_views();
        let revive_held = live.revive_held();
        let mut actions = coding::pickup::plan(coding::pickup::PickupInput {
            device_id: &self.pickup.device_id,
            sessions: rows,
            messages,
            live: &views,
            in_flight: &excluded,
            revive_held: &revive_held,
        });
        // EXP-1292: rows this host claimed whose launch failed transiently
        // (or whose claim answer was lost) read `running` — plan only
        // launches `queued` ones.
        actions.extend(live.retry_launches(&self.pickup.device_id, rows, &excluded));
        if actions.is_empty() {
            return;
        }
        let deps = Arc::new(self.pickup.deps());
        let foreground = foreground_start_held(&self.pickup.ctx.data_dir);
        for action in actions {
            use coding::pickup::PickupAction;
            let blocking = match &action {
                PickupAction::Launch { session } if foreground => {
                    log::debug!(
                        "pickup: session {} waits — a foreground start holds this machine",
                        session.id
                    );
                    continue;
                }
                PickupAction::Launch { .. }
                | PickupAction::Resume { .. }
                // EXP-1341: a revive resumes/launches (network + git).
                | PickupAction::Revive { .. }
                // EXP-1374: an interrupted run's continuation resumes.
                | PickupAction::Continue { .. }
                // EXP-849: a switch suspends + relaunches (network + git).
                | PickupAction::Switch { .. } => true,
                // A delivery to an engine that just finished wakes it.
                PickupAction::Deliver { session_id, .. } => !live.contains_live(session_id),
                _ => false,
            };
            if blocking {
                self.spawn_attempt(action, &deps);
            } else if let Err(err) = coding::pickup::execute(&action, &deps) {
                log::warn!("pickup: {err}");
            }
        }
    }

    /// Run one blocking pickup action off the worker, its row in flight; a
    /// failure backs the row off ([`PICKUP_RETRY`]), and either way the next
    /// pass runs as soon as it settles.
    fn spawn_attempt(
        &self,
        action: coding::pickup::PickupAction,
        deps: &Arc<coding::pickup::PickupDeps>,
    ) {
        let session_id = action_session_id(&action).to_string();
        let Some(guard) = self.pickup.begin(&session_id) else {
            return;
        };
        let pickup = Arc::clone(&self.pickup);
        let deps = Arc::clone(deps);
        let nudge = self.nudge.clone();
        let spawned = std::thread::Builder::new()
            .name("exp-pickup".to_string())
            .spawn(move || {
                let _guard = guard;
                let settled = match &action {
                    coding::pickup::PickupAction::Launch { session } => {
                        match coding::pickup::launch_queued(session, &deps) {
                            Ok(coding::pickup::LaunchQueued::Launched { .. }) => true,
                            // Refused + reported on the row: nothing to retry.
                            Ok(coding::pickup::LaunchQueued::Disabled { reason }) => {
                                log::warn!("pickup: session {session_id} refused: {reason}");
                                true
                            }
                            Ok(coding::pickup::LaunchQueued::Skipped { reason }) => {
                                log::info!("pickup: session {session_id} skipped ({reason})");
                                false
                            }
                            Err(err) => {
                                log::warn!("pickup: launch of {session_id} failed: {err}");
                                false
                            }
                        }
                    }
                    // A wake that handed nothing (another process holds the
                    // session — a foreground `code` — or the wake was refused
                    // for these messages) backs off too, or every settle
                    // would replan the same Resume at once.
                    coding::pickup::PickupAction::Resume { session, messages } => {
                        woke(coding::pickup::resume_session(session, messages, &deps))
                    }
                    coding::pickup::PickupAction::Deliver { session_id, messages } => {
                        woke(coding::pickup::deliver_messages(session_id, messages, &deps))
                    }
                    // EXP-1341: the registry backs a failed revive off on its
                    // own (`revive_held`), so either way it is settled here.
                    coding::pickup::PickupAction::Revive { session } => {
                        if let Err(err) = coding::pickup::revive_session(session, &deps) {
                            log::warn!("pickup: revive of {session_id}: {err}");
                        }
                        true
                    }
                    // EXP-1374: same for an interrupted run's continuation.
                    coding::pickup::PickupAction::Continue { session } => {
                        if let Err(err) = coding::pickup::continue_interrupted(session, &deps) {
                            log::warn!("pickup: continuation of {session_id}: {err}");
                        }
                        true
                    }
                    // EXP-1334: a switch (an account's, or a reconfigure) a
                    // turn in flight refuses is no failure: the turn's end
                    // (its busy=false row) re-plans it, so it waits only
                    // [`SWITCH_BUSY_RETRY`], never [`PICKUP_RETRY`], which
                    // would hold the session's deliveries, answers and
                    // kills with it.
                    coding::pickup::PickupAction::Switch { session, account, messages } => {
                        let kind = switch_kind(account);
                        match coding::pickup::switch_session(session, account, None, messages, &deps) {
                            Ok(outcome) => {
                                log::info!("pickup: {kind} of {session_id}: {outcome:?}");
                                true
                            }
                            Err(coding::pickup::SwitchError::Busy) => {
                                log::info!("pickup: {kind} of {session_id} waits for the turn to end");
                                pickup.back_off_for(&session_id, SWITCH_BUSY_RETRY);
                                true
                            }
                            Err(err) => {
                                log::warn!("pickup: {kind} of {session_id} failed: {err}");
                                false
                            }
                        }
                    }
                    other => match coding::pickup::execute(other, &deps) {
                        Ok(()) => true,
                        Err(err) => {
                            log::warn!("pickup: {err}");
                            false
                        }
                    },
                };
                if !settled {
                    pickup.back_off(&session_id);
                }
                let _ = nudge.send(DaemonWork::Pickup);
            });
        if let Err(err) = spawned {
            log::warn!("pickup: attempt thread failed to spawn: {err}");
        }
    }

    /// EXP-1292's startup reconcile, NEW-5's re-claim, once the shapes are
    /// synced: every row on this device reading `running` (or `busy`) with no
    /// engine behind it (a crash, a kill -9, a reboot) is taken back — NEVER
    /// `ended`:
    /// - a pending input → the pass's own Resume replays it;
    /// - mid-turn (`busy`) → [`coding::pickup::continue_after_restart`]
    ///   resumes the recorded run with a continuation turn;
    /// - between turns → `idle` (a persistent root is then the pass's
    ///   Revive; anything else waits for its next message).
    ///
    /// A row another process on this machine holds (a foreground `code`) is
    /// skipped by its claim lock (EXP-229).
    fn reconcile_rows(&self, rows: &[SessionRow], messages: &[MessageRow]) {
        let pending = pending_session_ids(messages);
        let left_behind = std::mem::take(&mut *lock_or_recover(&self.pickup.reclaiming));
        let mut stale = Vec::new();
        let mut unstarted = Vec::new();
        let mut mid_turn = Vec::new();
        for row in rows {
            let running = row.status_is(domain::contract::SESSION_STATUS_RUNNING);
            let idle = row.status_is(domain::contract::SESSION_STATUS_IDLE);
            if !row.on_device(&self.pickup.device_id)
                || !(running || (idle && (row.busy || left_behind.contains(&row.id))))
                || self.pickup.live.contains_live(&row.id)
            {
                continue;
            }
            if !matches!(
                coding::pickup::ClaimLock::try_acquire(&self.pickup.ctx.data_dir, &row.id),
                Ok(Some(_))
            ) {
                log::info!("reclaim: session {} runs in another process here — skipped", row.id);
                // A left-behind row a sibling still holds stays on the list
                // within the grace: that sibling may be the process being
                // replaced, its claim stale a few seconds later.
                if left_behind.contains(&row.id) && Instant::now() < self.pickup.reclaim_until {
                    lock_or_recover(&self.pickup.reclaiming).insert(row.id.clone());
                }
                continue;
            }
            let recorded = coding::run_registry::get(&self.pickup.ctx.data_dir, &row.id).is_some();
            match coding::pickup::reclaim_step(row, pending.contains(row.id.as_str()), recorded) {
                coding::pickup::ReclaimStep::Replay => {
                    log::info!("reclaim: session {} re-claimed — its pending messages replay", row.id);
                }
                coding::pickup::ReclaimStep::Continue => mid_turn.push(row.clone()),
                coding::pickup::ReclaimStep::Idle => {
                    log::info!("reclaim: session {} re-claimed between turns — idle", row.id);
                    stale.push(row.id.clone());
                }
                coding::pickup::ReclaimStep::Waiting => {
                    log::info!("reclaim: session {} re-claimed — idle, waiting for its next message", row.id);
                }
                coding::pickup::ReclaimStep::Unstarted => unstarted.push(row.id.clone()),
            }
        }
        if !mid_turn.is_empty() {
            let deps = Arc::new(self.pickup.deps());
            for row in mid_turn {
                // In flight BEFORE this pass plans: no Revive/Resume races it.
                let Some(guard) = self.pickup.begin(&row.id) else {
                    continue;
                };
                let deps = Arc::clone(&deps);
                let pickup = Arc::clone(&self.pickup);
                let nudge = self.nudge.clone();
                let spawned = std::thread::Builder::new()
                    .name("exp-reclaim".to_string())
                    .spawn(move || {
                        let _guard = guard;
                        let id = row.id.as_str();
                        match coding::pickup::continue_after_restart(&row, &deps) {
                            Ok(true) => log::info!(
                                "reclaim: session {id} was mid-turn — resumed with a continuation turn"
                            ),
                            Ok(false) => {
                                let result = api::session_device::report(
                                    &pickup.ctx.trpc,
                                    id,
                                    &idle_report(),
                                );
                                match result {
                                    Ok(()) => log::info!(
                                        "reclaim: session {id} was mid-turn with no run record here — now idle"
                                    ),
                                    Err(err) => log::warn!("reclaim: session {id} not reported idle: {err}"),
                                }
                            }
                            Err(err) => {
                                log::warn!("reclaim: session {id} not continued: {err}");
                                pickup.back_off(id);
                            }
                        }
                        let _ = nudge.send(DaemonWork::Pickup);
                    });
                if let Err(err) = spawned {
                    log::warn!("reclaim: continuation thread failed to spawn: {err}");
                }
            }
        }
        if stale.is_empty() && unstarted.is_empty() {
            return;
        }
        let trpc = Arc::clone(&self.pickup.ctx.trpc);
        std::thread::spawn(move || {
            for session_id in unstarted {
                coding::pickup::settle_unstarted(&trpc, &session_id);
            }
            for session_id in stale {
                if let Err(err) = api::session_device::report(&trpc, &session_id, &idle_report()) {
                    log::warn!("reclaim: session {session_id} not reported idle: {err}");
                }
            }
        });
    }

}

/// NEW-5: one sorted, de-duplicated `liveSessionIds` out of every source;
/// `None` past [`api::devices::LIVE_SESSION_IDS_CAP`] (the server would
/// refuse the whole beat — no claim beats a truncated one).
fn live_session_ids<I: IntoIterator<Item = String>>(
    sources: impl IntoIterator<Item = I>,
) -> Option<Vec<String>> {
    let ids: std::collections::BTreeSet<String> = sources.into_iter().flatten().collect();
    (ids.len() <= api::devices::LIVE_SESSION_IDS_CAP).then(|| ids.into_iter().collect())
}

/// EXP-1292: a session with no engine on this device — idle and not working.
fn idle_report() -> api::session_device::SessionReport {
    api::session_device::SessionReport {
        status: Some(api::session_device::ReportStatus::Idle),
        busy: Some(false),
        ..api::session_device::SessionReport::default()
    }
}

/// The sessions with an input waiting for them — a person's message or one
/// another session posted (a child's `report`, a child asking its parent):
/// [`coding::pickup::is_pending_input`], the pickup's own predicate, so the
/// reconcile's Resume reads the same rows.
fn pending_session_ids(messages: &[MessageRow]) -> HashSet<&str> {
    messages
        .iter()
        .filter(|message| coding::pickup::is_pending_input(message))
        .map(|message| message.session_id.as_str())
        .collect()
}

/// One synced `sessions` row, point-read by primary key (§5.8).
fn session_row(store: &sync::store::ShapeStore, session_id: &str) -> Option<SessionRow> {
    let spec = sync::shapes::shape_by_name("sessions")?;
    let row = store
        .read_by_key(spec, &sync::protocol::RowKey::Single(session_id.to_string()))
        .ok()??;
    serde_json::from_value(serde_json::Value::Object(row)).ok()
}

/// A root's linked issue identifiers in link order (`session_issues` ⋈
/// `issues`) — the branch name and the run's label. Issues not synced here
/// are skipped.
fn root_issue_identifiers(store: &sync::store::ShapeStore, root_id: &str) -> Vec<String> {
    let links = read_shape_rows::<domain::rows::SessionIssueRow>(store, "session_issues");
    ordered_issue_ids(links, root_id)
        .into_iter()
        .filter_map(|issue_id| issue_field(store, &issue_id, "identifier"))
        .collect()
}

/// A root's linked issue ids in `sort_order`, ties by
/// row id (pure, unit-tested).
fn ordered_issue_ids(
    links: Vec<domain::rows::SessionIssueRow>,
    root_id: &str,
) -> Vec<String> {
    let order = |link: &domain::rows::SessionIssueRow| link.sort_order.unwrap_or(0.0);
    let mut links: Vec<_> = links
        .into_iter()
        .filter(|link| link.session_id == root_id)
        .collect();
    links.sort_by(|a, b| order(a).total_cmp(&order(b)).then_with(|| a.id.cmp(&b.id)));
    links.into_iter().map(|link| link.issue_id).collect()
}

/// Hydrate a whole shape table into row structs (§5.5 tolerant decode — an
/// undecodable row is dropped, never fatal). A shape the sync crate has no
/// spec for reads empty.
pub(crate) fn read_shape_rows<T: serde::de::DeserializeOwned>(
    store: &sync::store::ShapeStore,
    shape: &str,
) -> Vec<T> {
    let Some(spec) = sync::shapes::shape_by_name(shape) else {
        return Vec::new();
    };
    match store.read_all(spec) {
        Ok(rows) => rows
            .into_iter()
            .filter_map(|row| serde_json::from_value(serde_json::Value::Object(row)).ok())
            .collect(),
        Err(err) => {
            log::debug!("daemon sync: reading the {shape} table failed: {err}");
            Vec::new()
        }
    }
}

/// NEW-8: every synced message KEYED through its conversation
/// (`domain::message_conversation`): `session_id` = the session its
/// conversation belongs to; a dm/channel message, or one whose conversation
/// has not synced, is held back. The row's own legacy `session_id` is never
/// read.
pub(crate) fn read_keyed_messages(store: &sync::store::ShapeStore) -> Vec<MessageRow> {
    let conversations = read_shape_rows::<domain::rows::ConversationRow>(store, "conversations");
    let sessions = domain::message_conversation::conversation_sessions(&conversations);
    let messages = read_shape_rows::<MessageRow>(store, "messages");
    domain::message_conversation::key_messages_by_session(&messages, &sessions)
}

/// One column of one issue, point-read by primary key (§5.8).
fn issue_field(store: &sync::store::ShapeStore, issue_id: &str, column: &str) -> Option<String> {
    let spec = sync::shapes::shape_by_name("issues")?;
    let row = store
        .read_by_key(spec, &sync::protocol::RowKey::Single(issue_id.to_string()))
        .ok()??;
    row.get(column)
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
}

// ---------------------------------------------------------------------------
// Service management
// ---------------------------------------------------------------------------

/// EXP-1099: where the service manager appends the daemon's stdout/stderr.
fn service_log_path(data_dir: &std::path::Path) -> PathBuf {
    coding::logging::logs_dir(data_dir).join("daemon-service.log")
}

fn xml_escape(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

fn service_exec() -> anyhow::Result<PathBuf> {
    super::update::running_exe().context("resolve the exponential binary path")
}

fn install(args: &[String]) -> CommandResult {
    let mut args = args.to_vec();
    // `--label` bakes the machine name into the service invocation.
    let label = take_value(&mut args, "--label").filter(|value| !value.is_empty());
    reject_unknown_flags(&args)?;
    // Fail fast while interactive instead of from inside the service.
    let _ = context::load()?;
    let exe = service_exec()?;
    // EXP-1111: a re-install (the installer re-run over a new binary)
    // restarts the running daemon onto it — unless that would kill live
    // agent sessions, which keep the old process until they end.
    let data_dir = context::data_dir();
    let previous = daemon_pid(&data_dir);
    let live = previous
        .map(|pid| registry::sessions_owned_by(&data_dir, pid))
        .unwrap_or(0);
    let keep_running = live > 0;
    // EXP-1099: the service's stdout/stderr go to a file under the data dir
    // (launchd drops them otherwise), so a crash trace survives; the regular
    // log lines are in the rotating `logs/exponential-daemon.log`.
    let service_log = service_log_path(&context::data_dir());
    if let Some(parent) = service_log.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let label_args_plist = label
        .as_deref()
        .map(|label| {
            format!(
                "\n    <string>--label</string>\n    <string>{}</string>",
                label.replace('&', "&amp;").replace('<', "&lt;")
            )
        })
        .unwrap_or_default();
    let label_args_unit = label
        .as_deref()
        .map(|label| format!(" --label \"{}\"", label.replace('"', "\\\"")))
        .unwrap_or_default();
    if cfg!(target_os = "macos") {
        let plist_dir = dirs::home_dir()
            .context("resolve home")?
            .join("Library/LaunchAgents");
        std::fs::create_dir_all(&plist_dir)?;
        let plist = plist_dir.join("at.exponential.cli.plist");
        std::fs::write(
            &plist,
            format!(
                r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key><string>at.exponential.cli</string>
  <key>ProgramArguments</key>
  <array>
    <string>{exe}</string>
    <string>daemon</string>
    <string>--foreground</string>{label_args_plist}
  </array>
  <key>RunAtLoad</key><true/>
  <key>KeepAlive</key><true/>
  <key>StandardOutPath</key><string>{log}</string>
  <key>StandardErrorPath</key><string>{log}</string>
</dict>
</plist>
"#,
                exe = exe.display(),
                log = xml_escape(&service_log.display().to_string())
            ),
        )?;
        println!("Wrote {}", plist.display());
        if keep_running {
            println!("{}", keep_running_notice(previous.unwrap_or_default(), live));
        } else {
            // Unload first (a no-op when it was never loaded): `load` on an
            // already-loaded label fails and would leave the old process.
            let _ = std::process::Command::new("launchctl")
                .args(["unload"])
                .arg(&plist)
                .stderr(std::process::Stdio::null())
                .status();
            let loaded = std::process::Command::new("launchctl")
                .args(["load", "-w"])
                .arg(&plist)
                .status()
                .map(|status| status.success())
                .unwrap_or(false);
            if loaded {
                println!("Daemon loaded — it starts at login from now on.");
            } else {
                println!("Load it with: launchctl load -w {}", plist.display());
            }
        }
    } else {
        // EXP-1111: linger FIRST — it also brings up the user manager a
        // headless SSH/provisioning shell may not have yet.
        ensure_linger();
        let unit_dir = dirs::config_dir()
            .context("resolve XDG config dir")?
            .join("systemd/user");
        std::fs::create_dir_all(&unit_dir)?;
        let unit = unit_dir.join("exponential-daemon.service");
        std::fs::write(
            &unit,
            format!(
                "[Unit]\nDescription=Exponential remote-start daemon\nAfter=network-online.target\n\n[Service]\nExecStart={exe} daemon --foreground{label_args_unit}\nRestart=on-failure\nRestartSec=5\nStandardOutput=append:{log}\nStandardError=append:{log}\n\n[Install]\nWantedBy=default.target\n",
                exe = exe.display(),
                log = service_log.display()
            ),
        )?;
        println!("Wrote {}", unit.display());
        let systemctl = |args: &[&str]| {
            std::process::Command::new("systemctl")
                .arg("--user")
                .args(args)
                .status()
                .map(|status| status.success())
                .unwrap_or(false)
        };
        let enabled = systemctl(&["daemon-reload"]) && systemctl(&["enable", "exponential-daemon"]);
        // `restart` starts a stopped unit too, and moves a running one onto
        // the new binary + unit.
        let started = enabled && (keep_running || systemctl(&["restart", "exponential-daemon"]));
        if started {
            println!("Daemon enabled (systemd user unit `exponential-daemon`, starts at boot).");
            if keep_running {
                println!("{}", keep_running_notice(previous.unwrap_or_default(), live));
            }
        } else {
            println!("Enable it with: systemctl --user daemon-reload && systemctl --user enable --now exponential-daemon");
        }
    }
    // EXP-1111: say whether it actually came up — the installer's summary
    // and a person at a terminal both need "running", not "unit written".
    match wait_for_daemon(&data_dir, if keep_running { None } else { previous }) {
        Some(pid) => println!("Daemon running (pid {pid}) — this machine is registered as a device."),
        None => println!(
            "The daemon is not running yet — check `exponential daemon status` and {}",
            service_log_path(&data_dir).display()
        ),
    }
    Ok(ExitCode::SUCCESS)
}

/// EXP-1111: the daemon keeps its process (live agent sessions) — what to do.
fn keep_running_notice(pid: u32, live: usize) -> String {
    format!(
        "The running daemon (pid {pid}) hosts {live} live session(s), so it was not restarted; it keeps the previous binary until you restart it: {}",
        restart_hint()
    )
}

/// Poll up to 15s for a daemon pid other than `replaced` (the process a
/// restart just stopped). At the deadline any live daemon counts: one
/// started by hand outside the service keeps running, and the service's
/// own instance defers to it.
fn wait_for_daemon(data_dir: &Path, replaced: Option<u32>) -> Option<u32> {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        if let Some(pid) = daemon_pid(data_dir).filter(|pid| Some(*pid) != replaced) {
            return Some(pid);
        }
        if Instant::now() >= deadline {
            return daemon_pid(data_dir);
        }
        std::thread::sleep(Duration::from_millis(300));
    }
}

/// The one-line instruction when this process may not enable lingering.
fn linger_hint(user: &str) -> String {
    format!("To keep the daemon running after logout and start it at boot, run once: sudo loginctl enable-linger {user}")
}

/// EXP-1111 (Linux): a systemd USER unit stops with the last login session
/// and never starts at boot unless the user lingers. Enable it when this
/// user may (polkit usually allows it for oneself; `--no-ask-password` keeps
/// a headless install from hanging on a prompt), else print the one line
/// that does it with sudo.
fn ensure_linger() {
    let user = std::env::var("USER")
        .ok()
        .filter(|user| !user.is_empty())
        .or_else(|| {
            std::process::Command::new("id")
                .arg("-un")
                .output()
                .ok()
                .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
                .filter(|user| !user.is_empty())
        });
    let Some(user) = user else {
        println!("{}", linger_hint("$USER"));
        return;
    };
    let lingering = match std::process::Command::new("loginctl")
        .args(["show-user", &user, "--property=Linger", "--value"])
        .stderr(std::process::Stdio::null())
        .output()
    {
        Ok(output) => String::from_utf8_lossy(&output.stdout).trim() == "yes",
        Err(_) => {
            println!("No loginctl on this machine: the daemon runs while you are logged in.");
            return;
        }
    };
    if lingering {
        return;
    }
    let enabled = std::process::Command::new("loginctl")
        .args(["--no-ask-password", "enable-linger", &user])
        .stderr(std::process::Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false);
    if enabled {
        println!("Enabled lingering for {user}: the daemon keeps running after logout and starts at boot.");
    } else {
        println!("{}", linger_hint(&user));
    }
}

fn uninstall(args: &[String]) -> CommandResult {
    reject_unknown_flags(args)?;
    remove_service()?;
    Ok(ExitCode::SUCCESS)
}

/// EXP-641: restart the installed service so a daemon it supervises picks up
/// a freshly installed binary (`exponential update` swaps the file; the
/// running process keeps the old inode until it re-execs). `Ok(false)` when
/// no service is installed — the caller tells the user to restart by hand.
pub fn restart_service() -> anyhow::Result<bool> {
    if cfg!(target_os = "macos") {
        let plist = dirs::home_dir()
            .context("resolve home")?
            .join("Library/LaunchAgents/at.exponential.cli.plist");
        if !plist.exists() {
            return Ok(false);
        }
        // `kickstart -k` restarts a loaded launchd service in place.
        let target = format!("gui/{}/at.exponential.cli", unsafe { libc::getuid() });
        let status = std::process::Command::new("launchctl")
            .args(["kickstart", "-k", &target])
            .status()
            .context("run launchctl kickstart")?;
        if !status.success() {
            bail!("launchctl kickstart -k {target} exited with {status}");
        }
        Ok(true)
    } else {
        let unit = dirs::config_dir()
            .context("resolve XDG config dir")?
            .join("systemd/user/exponential-daemon.service");
        if !unit.exists() {
            return Ok(false);
        }
        let status = std::process::Command::new("systemctl")
            .args(["--user", "restart", "exponential-daemon"])
            .status()
            .context("run systemctl --user restart")?;
        if !status.success() {
            bail!("systemctl --user restart exponential-daemon exited with {status}");
        }
        Ok(true)
    }
}

/// The manual restart command for this platform's service (printed when
/// [`restart_service`] cannot do it).
pub fn restart_hint() -> &'static str {
    if cfg!(target_os = "macos") {
        "launchctl kickstart -k gui/$(id -u)/at.exponential.cli"
    } else {
        "systemctl --user restart exponential-daemon"
    }
}

/// Stop and remove the launchd agent / systemd user unit. Shared with the
/// top-level `uninstall`, which removes the service before the binary.
pub fn remove_service() -> anyhow::Result<()> {
    if cfg!(target_os = "macos") {
        let plist = dirs::home_dir()
            .context("resolve home")?
            .join("Library/LaunchAgents/at.exponential.cli.plist");
        let _ = std::process::Command::new("launchctl")
            .args(["unload", "-w"])
            .arg(&plist)
            .status();
        if plist.exists() {
            std::fs::remove_file(&plist)?;
            println!("Removed {}", plist.display());
        } else {
            println!("No launch agent installed.");
        }
    } else {
        let _ = std::process::Command::new("systemctl")
            .args(["--user", "disable", "--now", "exponential-daemon"])
            .status();
        let unit = dirs::config_dir()
            .context("resolve XDG config dir")?
            .join("systemd/user/exponential-daemon.service");
        if unit.exists() {
            std::fs::remove_file(&unit)?;
            println!("Removed {}", unit.display());
        } else {
            println!("No systemd unit installed.");
        }
    }
    Ok(())
}

fn status(args: &[String]) -> CommandResult {
    reject_unknown_flags(args)?;
    let data_dir = context::data_dir();
    match daemon_pid(&data_dir) {
        Some(pid) => {
            println!("Daemon running (pid {pid}).");
            Ok(ExitCode::SUCCESS)
        }
        None => {
            println!("Daemon not running.");
            println!("{}", super::account::NOT_REGISTERED_HINT);
            Ok(ExitCode::FAILURE)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// EXP-1353: the daemon offers every clone to the shared GC, and the
    /// shared throttle keeps a second pass within the hour from re-walking.
    /// NEW-5: the heartbeat's `liveSessionIds` = the union of every live
    /// source, sorted and de-duplicated; past the cap no claim at all.
    #[test]
    fn heartbeat_live_ids_union_the_live_runs() {
        let ids = live_session_ids([
            vec!["b".to_string(), "a".to_string()],
            vec!["a".to_string()],
            Vec::new(),
            vec!["c".to_string()],
        ]);
        assert_eq!(ids, Some(vec!["a".to_string(), "b".to_string(), "c".to_string()]));
        assert_eq!(live_session_ids(Vec::<Vec<String>>::new()), Some(Vec::new()));
        let many: Vec<String> =
            (0..=api::devices::LIVE_SESSION_IDS_CAP).map(|n| format!("s{n}")).collect();
        assert_eq!(live_session_ids([many]), None);
    }

    #[test]
    fn sweep_cargo_targets_walks_each_clone_once_per_interval() {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("exp-cli-gc-{}-{nanos}", std::process::id()));
        for name in ["acme/web", "acme/api"] {
            std::fs::create_dir_all(root.join(name).join(".git")).unwrap();
        }
        // A bare sibling is no clone.
        std::fs::create_dir_all(root.join("acme/web.cargo-target")).unwrap();
        let first = sweep_cargo_targets(&root);
        let second = sweep_cargo_targets(&root);
        let _ = std::fs::remove_dir_all(&root);
        assert_eq!(first, vec!["acme/api".to_string(), "acme/web".to_string()]);
        assert!(second.is_empty());
    }

    fn advert(agents: &[&str]) -> coding::AgentAdvertisement {
        coding::AgentAdvertisement {
            agents: agents.iter().map(|agent| agent.to_string()).collect(),
            unauthed_agents: Vec::new(),
            default_agent: "claude".to_string(),
            launch_defaults: agents
                .iter()
                .map(|agent| (agent.to_string(), coding::AgentLaunchDefaults::default()))
                .collect(),
            acp_agents: Vec::new(),
        }
    }

    /// FEED-36: the idle gate — nothing live applies now, sessions idle
    /// past the grace get ended, one working (or freshly idle) run waits.
    #[test]
    fn a_pending_update_ends_only_sessions_idle_past_the_grace() {
        let grace = UPDATE_IDLE_GRACE;
        assert_eq!(update_gate([], grace), UpdateGate::Apply);
        assert_eq!(update_gate([Some(grace)], grace), UpdateGate::EndIdle);
        assert_eq!(update_gate([Some(grace * 3), Some(grace)], grace), UpdateGate::EndIdle);
        assert_eq!(
            update_gate([Some(grace), Some(grace - Duration::from_secs(1))], grace),
            UpdateGate::Wait
        );
        assert_eq!(update_gate([Some(grace), None], grace), UpdateGate::Wait);
        assert_eq!(update_gate([None], grace), UpdateGate::Wait);
        assert_eq!(update_gate([Some(Duration::ZERO)], grace), UpdateGate::Wait);
    }

    #[test]
    fn gated_update_is_immediate_then_paced() {
        let now = Instant::now();
        // Not gated: never.
        assert!(!gated_update_due(false, None, now));
        assert!(!gated_update_due(false, Some(now), now + GATED_UPDATE_RETRY * 2));
        // Gated, never attempted: right away — no 6h throttle to wait out.
        assert!(gated_update_due(true, None, now));
        // Gated, attempted just now: wait for the retry cadence.
        assert!(!gated_update_due(true, Some(now), now));
        assert!(!gated_update_due(
            true,
            Some(now),
            now + GATED_UPDATE_RETRY - Duration::from_secs(1)
        ));
        assert!(gated_update_due(true, Some(now), now + GATED_UPDATE_RETRY));
    }

    #[test]
    fn a_matching_probe_keeps_and_clears_any_pending_change() {
        let current = advert(&["claude"]);
        let mut pending = Some(advert(&["claude", "codex"]));
        assert_eq!(
            advert_transition(&current, &current.clone(), &mut pending),
            AdvertStep::Keep
        );
        assert!(pending.is_none(), "a flap back to current clears the pending change");
    }

    #[test]
    fn a_first_disagreeing_probe_only_arms_confirmation() {
        let current = advert(&["claude"]);
        let observed = advert(&[]);
        let mut pending = None;
        assert_eq!(
            advert_transition(&current, &observed, &mut pending),
            AdvertStep::AwaitConfirmation
        );
        assert_eq!(pending, Some(observed));
    }

    #[test]
    fn a_confirmed_change_applies_and_clears_pending() {
        let current = advert(&["claude"]);
        let observed = advert(&["claude", "codex"]);
        let mut pending = Some(observed.clone());
        assert_eq!(
            advert_transition(&current, &observed, &mut pending),
            AdvertStep::Apply
        );
        assert!(pending.is_none());
    }

    #[test]
    fn a_wobble_never_touches_the_channel() {
        // A → B → A: the flaky probe pattern that was flapping presence.
        let current = advert(&["claude"]);
        let wobble = advert(&[]);
        let mut pending = None;
        assert_eq!(
            advert_transition(&current, &wobble, &mut pending),
            AdvertStep::AwaitConfirmation
        );
        assert_eq!(
            advert_transition(&current, &current.clone(), &mut pending),
            AdvertStep::Keep
        );
        assert!(pending.is_none());
        // The next wobble starts confirmation from scratch again.
        assert_eq!(
            advert_transition(&current, &wobble, &mut pending),
            AdvertStep::AwaitConfirmation
        );
    }

    /// EXP-437: a defaults-only settings edit (same agent sets, different
    /// model/toggles) is a real advertisement change — it walks the same
    /// AwaitConfirmation → Apply damping as an install/uninstall.
    #[test]
    fn a_defaults_only_change_confirms_and_applies() {
        let current = advert(&["claude"]);
        let mut observed = advert(&["claude"]);
        observed
            .launch_defaults
            .insert(
                "claude".to_string(),
                coding::AgentLaunchDefaults {
                    model: "opus".to_string(),
                    plan_mode: true,
                    ..coding::AgentLaunchDefaults::default()
                },
            );
        let mut pending = None;
        assert_eq!(
            advert_transition(&current, &observed, &mut pending),
            AdvertStep::AwaitConfirmation
        );
        assert_eq!(
            advert_transition(&current, &observed, &mut pending),
            AdvertStep::Apply
        );
        assert!(pending.is_none());
    }

    #[test]
    fn a_different_second_change_replaces_the_pending_value() {
        let current = advert(&["claude"]);
        let first = advert(&[]);
        let second = advert(&["codex"]);
        let mut pending = None;
        advert_transition(&current, &first, &mut pending);
        assert_eq!(
            advert_transition(&current, &second, &mut pending),
            AdvertStep::AwaitConfirmation
        );
        assert_eq!(pending, Some(second));
    }

    fn thread_record(cwd: &Path, session_id: &str) -> coding::run_registry::RunRecord {
        coding::run_registry::RunRecord {
            session_id: session_id.to_string(),
            account_id: "acct-1".to_string(),
            agent: coding::CodingAgent::Claude,
            kind: coding::run_registry::RunKind::Thread,
            team_id: "team-1".to_string(),
            issues: vec!["EXP-42".to_string()],
            title: None,
            cwd: cwd.to_path_buf(),
            clone: None,
            repo: None,
            repository_id: None,
            branch: Some("exp/EXP-42".to_string()),
            base_branch: Some("master".to_string()),
            claude_session_id: Some("claude-1".to_string()),
            codex_originator: None,
            model: String::new(),
            effort: String::new(),
            ultracode: false,
            transport: None,
            acp_session_id: None,
            agent_native_session_id: None,
            acp_child_pid: None,
            host_pid: None,
            recorded_at: coding::run_registry::now_secs(),
            extra: std::collections::BTreeMap::new(),
        }
    }

    /// EXP-862 review nit: the accounts a live run is on come off ONE
    /// registry load per `agent_profile_remove`, and the result is what a
    /// per-id `get` produced — `session_ids` order, ambient (account-less)
    /// runs and unknown ids skipped, a re-recorded run (`record` upserts by
    /// id, the EXP-866 mid-run switch) resolving to its NEWEST account.
    #[test]
    fn live_run_accounts_resolve_through_one_registry_load() {
        let dir = std::env::temp_dir().join(format!("exp-cli-live-accounts-{}", uuid::Uuid::new_v4()));
        let cwd = dir.join("worktree");
        std::fs::create_dir_all(&cwd).expect("temp worktree");
        let with_account = |session_id: &str, account: Option<&str>| {
            let mut record = thread_record(&cwd, session_id);
            record.set_account(account);
            record
        };
        coding::run_registry::record(&dir, with_account("sess-a", Some("work")));
        coding::run_registry::record(&dir, with_account("sess-b", None));
        coding::run_registry::record(&dir, with_account("sess-c", Some("personal")));
        coding::run_registry::record(&dir, with_account("sess-a", Some("shadow")));

        let ids = |ids: &[&str]| ids.iter().map(|id| id.to_string()).collect::<Vec<_>>();
        let pair = |account: &str| (coding::CodingAgent::Claude, account.to_string());
        // A legacy ambient run holds no profile: skipped.
        assert_eq!(
            live_run_accounts(&dir, &ids(&["sess-c", "sess-b", "sess-a", "sess-missing"])),
            vec![pair("personal"), pair("shadow")]
        );
        assert!(live_run_accounts(&dir, &[]).is_empty());
        assert!(live_run_accounts(&dir, &ids(&["sess-b"])).is_empty());

        let _ = std::fs::remove_dir_all(&dir);
    }

    // -----------------------------------------------------------------------
    // EXP-1292: the pickup host's pure helpers
    // -----------------------------------------------------------------------

    /// A row on `session_id`'s conversation, already keyed (NEW-8).
    fn message(id: &str, session_id: &str, consumed: bool) -> MessageRow {
        let row: MessageRow = serde_json::from_value(serde_json::json!({
            "id": id,
            "conversation_id": format!("c-{session_id}"),
            "author_kind": "user",
            "body": "hi",
            "consumed_at": consumed.then_some("2026-10-13 10:00:00+00"),
        }))
        .expect("message row decodes");
        MessageRow { session_id: session_id.to_string(), ..row }
    }

    /// An agent-authored row on `session_id`: `from` = the session that
    /// posted it (a child's `report`), `None` = the session's own reply.
    fn agent_message(id: &str, session_id: &str, from: Option<&str>) -> MessageRow {
        let row: MessageRow = serde_json::from_value(serde_json::json!({
            "id": id,
            "conversation_id": format!("c-{session_id}"),
            "author_kind": "agent",
            "author_session_id": from,
            "body": "done",
            "consumed_at": null,
        }))
        .expect("message row decodes");
        MessageRow { session_id: session_id.to_string(), ..row }
    }

    /// EXP-1329: a child's pending `report` is an input like a person's
    /// message — it marks the row for the reconcile's Resume; the session's
    /// own reply never does.
    #[test]
    fn pending_inputs_count_child_reports_never_own_replies() {
        let messages = vec![
            message("m-person", "s-person", false),
            message("m-read", "s-read", true),
            agent_message("m-report", "s-reported", Some("child-1")),
            agent_message("m-own", "s-own-reply", None),
            agent_message("m-self", "s-self", Some("s-self")),
        ];
        let mut pending: Vec<&str> = pending_session_ids(&messages).into_iter().collect();
        pending.sort_unstable();
        assert_eq!(pending, vec!["s-person", "s-reported"]);
    }

    /// The branch and label follow the thread's link order — `sort_order`
    /// as a number, never as text; other roots' links never leak in.
    #[test]
    fn a_roots_issues_come_in_link_order() {
        let link = |id: &str, session: &str, issue: &str, order: serde_json::Value| {
            serde_json::from_value::<domain::rows::SessionIssueRow>(serde_json::json!({
                "id": id, "session_id": session, "issue_id": issue, "sort_order": order,
            }))
            .expect("link decodes")
        };
        let links = vec![
            link("l1", "root", "i-ten", serde_json::json!("10")),
            link("l2", "root", "i-two", serde_json::json!("2")),
            link("l3", "other", "i-x", serde_json::json!("1")),
            link("l4", "root", "i-three", serde_json::json!(3.0)),
        ];
        assert_eq!(ordered_issue_ids(links, "root"), vec!["i-two", "i-three", "i-ten"]);
    }

    /// The pickup's input shapes wake the pass, everything else is ignored
    /// (NEW-2: no event feed: the server fires agent triggers).
    #[test]
    fn deltas_route_to_the_worker() {
        assert_eq!(delta_work("sessions"), Some(DaemonWork::Pickup));
        assert_eq!(delta_work("messages"), Some(DaemonWork::Pickup));
        assert_eq!(delta_work("conversations"), Some(DaemonWork::Pickup));
        assert_eq!(delta_work("session_issues"), Some(DaemonWork::Pickup));
        assert_eq!(delta_work("issue_events"), None);
        assert_eq!(delta_work("labels"), None);
    }

    /// The pickup never plans over a cold store: it waits for ALL of its
    /// shapes' first `up-to-date`.
    #[test]
    fn the_pickup_waits_for_its_shapes_first_sync() {
        let synced = SyncedShapes::default();
        assert!(!synced.ready(PICKUP_SHAPES));
        assert!(synced.mark("sessions"));
        assert!(!synced.mark("sessions"), "only the first up-to-date counts");
        assert!(!synced.ready(PICKUP_SHAPES));
        synced.mark("messages");
        assert!(!synced.ready(PICKUP_SHAPES), "NEW-8: the messages wait on their conversations");
        synced.mark("conversations");
        assert!(synced.ready(PICKUP_SHAPES));
    }

    /// One attempt per session at a time; a panicking attempt still lets go.
    #[test]
    fn an_in_flight_session_takes_no_second_attempt() {
        let set = Arc::new(Mutex::new(HashSet::new()));
        let begin = |id: &str| {
            lock_or_recover(&set).insert(id.to_string()).then(|| InFlight {
                set: Arc::clone(&set),
                session_id: id.to_string(),
            })
        };
        let first = begin("s1").expect("first attempt");
        assert!(begin("s1").is_none());
        assert!(begin("s2").is_some(), "other sessions are independent");
        drop(first);
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _guard = begin("s1").expect("free again");
            panic!("prepare blew up");
        }));
        assert!(begin("s1").is_some(), "a crashed attempt must not wedge its row");
    }

    /// EXP-1292: a wake that handed nothing (the wake was refused) is not
    /// settled, so its row backs off instead of replanning at once; NEW-45:
    /// a claim another process holds backs off the same way (it retries).
    #[test]
    fn a_wake_that_handed_nothing_backs_off() {
        assert!(woke(Ok(vec!["m1".to_string()])));
        assert!(!woke(Ok(Vec::new())));
        assert!(!woke(Err(coding::pickup::WakeError::Other("brief failed".to_string()))));
        assert!(!woke(Err(coding::pickup::WakeError::Contended)));
    }

    /// A foreground `code`/`run` mid-start holds the gate the pass checks.
    #[test]
    fn a_foreground_start_parks_the_daemons_launches() {
        let dir = std::env::temp_dir().join(format!("exp-cli-fg-gate-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        assert!(!foreground_start_held(&dir));
        let held = coding::pickup::ClaimLock::try_acquire(&dir, launch::FOREGROUND_START_LOCK)
            .unwrap()
            .expect("the foreground takes it");
        assert!(foreground_start_held(&dir));
        drop(held);
        assert!(!foreground_start_held(&dir));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// EXP-746 (D9): the cap LISTS and their three tests moved into
    /// `coding::doctor` — one const for both hosts. What stays here is the
    /// call-through: this daemon must advertise exactly that shared list.
    #[test]
    fn daemon_advertises_the_shared_caps() {
        assert_eq!(
            device_caps(&advert(&["claude"])),
            coding::device_caps(&advert(&["claude"]))
        );
        let caps = device_caps(&advert(&["claude"]));
        for cap in coding::DEVICE_CAPS {
            assert!(caps.contains(&cap.to_string()), "missing build cap {cap}");
        }
        for cap in coding::RUN_CAPS {
            assert!(caps.contains(&cap.to_string()), "missing run cap {cap}");
        }
        // Nothing runnable = build caps only.
        let signed_out = device_caps(&advert(&[]));
        assert_eq!(signed_out.len(), coding::DEVICE_CAPS.len());
        assert!(signed_out.contains(&"agent-login".to_string()));
        assert!(!signed_out.contains(&"triggers".to_string()));
        assert!(!signed_out.contains(&"automations".to_string()), "nor the legacy trigger cap");
    }

    /// The daemon's shapes are real shapes (the v2 pickup trio once
    /// #EXP-1294's specs land — until then the pipeline skips them), and
    /// deliberately NOT the desktop's whole set.
    #[test]
    fn daemon_shapes_are_a_real_subset() {
        assert!(DAEMON_SHAPES.len() < 20);
        for shape in DAEMON_SHAPES {
            let pending_v2 = ["sessions", "messages", "session_issues"].contains(shape);
            assert!(
                pending_v2 || sync::shapes::shape_by_name(shape).is_some(),
                "{shape} is not a real shape"
            );
        }
        for shape in PICKUP_SHAPES {
            assert!(DAEMON_SHAPES.contains(shape));
        }
    }
}
