//! `exponential daemon` — register this machine as a persistent per-user
//! device and execute remote starts: the headless twin of the desktop's
//! steer wiring. One control channel to the relay (dialed only while at
//! least one agent CLI is installed — EXP-367), `devices.register` +
//! periodic heartbeat for the durable registry row, and the same launch
//! path `code`/`run` use for every `start_session` frame (issue, batch,
//! action). `daemon install|uninstall|status` manage a systemd user unit
//! (Linux) / launchd agent (macOS).

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{bail, Context as _};
use coding::{LaunchOptions, Prepared, PrepareRequest};
use steer::control_channel::StartSessionFn;
use steer::{ControlApi, DeviceIdentity, RemoteStart, RemoteStartSubject, TrpcControlApi};

use super::{reject_unknown_flags, take_flag, take_value, CommandResult};
use crate::context::{self, Ctx};
use crate::launch::{self, ActionRepo};
use crate::registry;
use crate::session_host::{self, LaunchEnv, RunningSession};

/// EXP-481: 30s (down from 60) — online-ness now derives from
/// `last_seen_at` freshness against the contract's 90s window, so one
/// missed beat must not flap the badge.
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(30);
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
const DOCTOR_RECHECK: Duration = Duration::from_secs(5 * 60);
/// EXP-414: a changed agent advertisement is only ACTED on once a second
/// probe agrees ([`advert_transition`]) — this is the shortened recheck that
/// confirms (or clears) a pending change, so a real change still converges
/// in ~DOCTOR_RECHECK + this instead of two full periods.
const ADVERT_CONFIRM_RECHECK: Duration = Duration::from_secs(30);
/// EXP-746 (D9): the caps this daemon advertises — ONE list, owned by
/// [`coding::doctor::DEVICE_CAPS`] + [`coding::doctor::ACTION_CAPS`] and
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
    /// trigger it still waits for idle — but a gated daemon does not stay
    /// busy forever: since EXP-681 each hosted run's kill poll ends the run
    /// itself once the gate has outlasted the server sweep window
    /// (`session_host::GATED_KILL_AFTER`), so a forgotten person-started
    /// run (EXP-674: no idle reaper) can no longer pin a gated build.
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
struct LiveSession {
    issue_id: Option<String>,
    /// FEED-47/57: the issues a BATCH run covers (empty otherwise), so an
    /// issue start beside it is refused like one beside the issue's own run.
    batch_issue_ids: Vec<String>,
    /// EXP-530: the `actions` row this run executes (from the prepared
    /// launch's `action_id`) — the automation host's defer check ("never
    /// launch a second run of an action already running here").
    action_id: Option<String>,
    branch: String,
    is_fix_run: bool,
    /// EXP-637: the run's own worktree, reclaimed by the reaper when the
    /// session finishes clean. `None` for issue/batch sessions (their
    /// worktrees survive by design) and repo-less runs.
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

/// Local one-session-per-issue dedup — only STILL-RUNNING sessions count
/// (a finished entry awaiting the reaper tick must not block a restart).
fn issue_is_coding_here(sessions: &Sessions, issue_id: &str) -> bool {
    issue_run_here(sessions, issue_id).is_some()
}

/// [`issue_is_coding_here`], naming the live run that holds the issue
/// (FEED-63: the refusal reported back to the requester names it).
fn issue_run_here(sessions: &Sessions, issue_id: &str) -> Option<String> {
    lock_sessions(sessions)
        .iter()
        .find(|live| {
            covers_issue(live.issue_id.as_deref(), &live.batch_issue_ids, issue_id)
                && !live.session.is_done()
        })
        .map(|live| live.session.session_id.clone())
}

/// FEED-47/57: a run holds an issue when it IS the issue's run or a batch
/// run covering it.
fn covers_issue(run_issue: Option<&str>, batch_issue_ids: &[String], issue_id: &str) -> bool {
    run_issue == Some(issue_id) || batch_issue_ids.iter().any(|id| id == issue_id)
}

/// REV-9: in-flight remote-start reservations. The handler's dedup checks
/// (`issue_is_coding_here`, the server `live_for_issue` probe) only see a
/// session once prepare has FINISHED — the sessions vec is pushed after
/// `session_host::launch` returns and the server row is prepare's step 6,
/// both seconds (minutes on a first clone) after the frame arrived. Starts
/// are never acked, so a phone that observes nothing retries, and the
/// duplicate frame's own thread passes both checks and spawns a second agent
/// into the SAME `exp/<ID>` worktree. Each frame therefore atomically claims
/// its subject keys ON THE RUN LOOP, before its handler thread spawns; a
/// frame that fails to claim is dropped, and a claim is released only when
/// the handler returns — by which point the LiveSession is pushed, so one of
/// the two guards always covers a live start.
#[derive(Clone, Default)]
struct StartReservations(Arc<Mutex<HashSet<String>>>);

impl StartReservations {
    /// All-or-nothing claim: `Err(clashing key)` — holding NOTHING — when any
    /// key is already held by an in-flight start.
    fn claim(&self, keys: Vec<String>) -> Result<ReservationGuard, String> {
        let mut held = match self.0.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        if let Some(clash) = keys.iter().find(|key| held.contains(*key)) {
            return Err(clash.clone());
        }
        held.extend(keys.iter().cloned());
        Ok(ReservationGuard { held: Arc::clone(&self.0), keys })
    }
}

/// Releases its keys on drop — including a handler panic (a poisoned-lock
/// claim recovers via `into_inner`, so a crashed start never wedges its
/// issue until restart).
struct ReservationGuard {
    held: Arc<Mutex<HashSet<String>>>,
    keys: Vec<String>,
}

impl Drop for ReservationGuard {
    fn drop(&mut self) {
        let mut held = match self.held.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        for key in &self.keys {
            held.remove(key);
        }
    }
}

/// The subject keys one `start_session` frame must hold: every issue an
/// issue/batch start would put an agent on (an overlapping batch and single
/// start contend on the shared issue), the action id for an action start (a
/// doubled fix-conflicts frame would otherwise race `take_over_branch`'s
/// holder scan into the same PR worktree).
fn reservation_keys(subject: &RemoteStartSubject) -> Vec<String> {
    match subject {
        RemoteStartSubject::Issue(issue_id) => vec![format!("issue:{issue_id}")],
        RemoteStartSubject::Batch { issue_ids, .. } => {
            issue_ids.iter().map(|id| format!("issue:{id}")).collect()
        }
        RemoteStartSubject::Action { action_id, .. } => vec![format!("action:{action_id}")],
        // EXP-637: a retried resume frame must not relaunch the run twice.
        RemoteStartSubject::Resume { session_id } => vec![format!("resume:{session_id}")],
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

    // EXP-746: ONE registry decision for every end this process issues —
    // the PTY supervisor's and the ACP engine's alike. Installed before any
    // session can launch (first caller wins, process-wide).
    registry::install_end_observer(ctx.data_dir.clone());

    // EXP-229 parity: end orphaned rows a previous crash left `running`
    // (pid-guarded — rows owned by a live sibling process are skipped).
    reconcile_stale_sessions(&ctx);
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
            log::warn!("steer runtime failed to start (remote start disabled): {err}");
            None
        }
    };
    let personal_key = context::ensure_personal_key(&ctx).ok();
    let sessions: Sessions = Arc::new(Mutex::new(Vec::new()));
    let reservations = StartReservations::default();
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
    // EXP-530: the automation host — this daemon's own Electric pipeline (a
    // 6-shape subset in `sync-cli.sqlite`, never the GUI's store), ONE delta
    // drain nudging the serialized worker, plus the 30s self-tick below.
    // Sync failing to open is not fatal: everything else (remote starts,
    // heartbeat, worktrees) keeps working, automations just stay dormant.
    // EXP-641: raised by every path the server can 426 (the sync pipeline's
    // upgrade hook, the heartbeat) — the loop below turns it into an
    // immediate, throttle-free self-update attempt.
    let gated = Arc::new(AtomicBool::new(false));
    let sync_manager = start_automation_sync(&ctx, &gated);
    let automation_worker = sync_manager.as_ref().map(|manager| {
        let worker = spawn_automation_worker(AutomationHost {
            ctx: Arc::clone(&ctx),
            runtime: runtime.clone(),
            sessions: Arc::clone(&sessions),
            reservations: reservations.clone(),
            personal_key: personal_key.clone(),
            sync: Arc::clone(manager),
            device_id: device_id.clone(),
            event_cache: None,
        });
        spawn_delta_drain(manager, worker.clone());
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

    let (inbox_tx, inbox_rx) = flume::unbounded::<RemoteStart>();
    let mut control = runtime.as_ref().and_then(|runtime| {
        dial_control(runtime, &ctx, &device_id, &device_label, &advertised, &inbox_tx, &check_in)
    });
    if advertised.nothing_installed() {
        log::info!("no agent CLI installed — registered offline; install claude or codex to accept remote starts");
    } else if advertised.agents.is_empty() {
        log::info!(
            "no agent CLI signed in ({} installed but signed out) — remote starts will be refused until one is",
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
    // EXP-530: the automation beat. Event triggers ride the delta drain
    // (they fire within a second of the row landing); this tick is what
    // makes SCHEDULES fire — and the catch-up beat after a sleep/offline
    // stretch, where no delta ever arrives.
    let mut last_automation_tick = Instant::now();
    // EXP-1005: the account-rotation wall beat (every ROTATION_BEAT).
    let mut rotation = RotationHost {
        ctx: Arc::clone(&ctx),
        runtime: runtime.clone(),
        sessions: Arc::clone(&sessions),
        personal_key: personal_key.clone(),
        reservations: reservations.clone(),
        // Persisted: an auto-update re-exec keeps every chain's cap.
        tracker: Arc::new(Mutex::new(coding::account_rotation::RotationTracker::load(
            &ctx.data_dir,
            chrono::Utc::now().timestamp_millis(),
        ))),
        inflight: Arc::new(Mutex::new(coding::account_rotation::InflightProbes::new())),
        holds: HoldLog::default(),
        last_beat: Instant::now(),
    };
    while !shutdown_requested() {
        match inbox_rx.recv_timeout(Duration::from_secs(1)) {
            // REV-9: claim the frame's subject BEFORE spawning its thread —
            // the claim is the only dedup a duplicate frame can hit while the
            // first is still preparing (see [`StartReservations`]).
            Ok(start) => match reservations.claim(reservation_keys(&start.subject)) {
                Err(clash) => {
                    // EXP-758: before FEED-63 the sender heard NOTHING. A start
                    // frame has no reply on the control socket (there is no
                    // ack/refusal variant in `steer::frames::ClientFrame`, by
                    // design: "the remote client observes success purely via
                    // the synced `coding_sessions` row appearing"), and the
                    // daemon has no device-notice channel either: the
                    // heartbeat carries no message field and
                    // `devices.completeCommand` only answers PULLED commands,
                    // which starts are not. So a double-click on Start looks
                    // like a start that vanished. The drop is deliberate
                    // (REV-9's dedup); FEED-63 ends the silence below.
                    log::warn!(
                        "remote start dropped: a start holding {clash} is already in flight on this device"
                    );
                    // FEED-63: the sender hears it now, through the server,
                    // when the frame named its start. Off the 1Hz loop: the
                    // report is an HTTP round trip.
                    if let Some(start_id) = start.start_id.clone() {
                        let ctx = Arc::clone(&ctx);
                        std::thread::spawn(move || {
                            steer::report_start_failure(
                                &ctx.trpc,
                                Some(&start_id),
                                steer::START_DUPLICATE_REASON,
                            );
                        });
                    }
                }
                Ok(reservation) => {
                    let ctx = Arc::clone(&ctx);
                    let runtime = runtime.clone();
                    let sessions = Arc::clone(&sessions);
                    let personal_key = personal_key.clone();
                    let device_id = device_id.clone();
                    std::thread::spawn(move || {
                        let _reservation = reservation;
                        handle_remote_start(
                            &ctx,
                            runtime.as_ref(),
                            &sessions,
                            personal_key,
                            &device_id,
                            start,
                        );
                    });
                }
            },
            Err(flume::RecvTimeoutError::Timeout) => {}
            Err(flume::RecvTimeoutError::Disconnected) => break,
        }

        // EXP-637: a finished RUN reclaims its own worktree — but only when
        // it is provably clean and carries no commits. Blocking git on the
        // 1Hz loop is fine: it runs once per finished run, not per tick.
        // EXP-764: a finished repo-LESS run has no worktree to judge — it is
        // purged whole: scratch dir, claude trust entries, run record and
        // steer journal. Nothing of it is resumable.
        {
            let mut guard = lock_sessions(&sessions);
            let reaped: Vec<(String, Option<coding::RunCleanup>, PathBuf)> = guard
                .iter()
                .filter(|live| live.session.is_done())
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
                        let verdict = coding::remove_if_clean(&cleanup, &live);
                        log::info!(
                            "run cleanup [{session_id}] on {}: {verdict:?}",
                            cleanup.branch
                        );
                    }
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
            match api::devices::heartbeat(
                &ctx.trpc,
                &api::devices::HeartbeatInput {
                    device_id: &device_id,
                    active_sessions: live_now as u32,
                    defaults_synced_at: synced_at.as_deref(),
                    agent_accounts: send_accounts.then_some(accounts_json.as_ref()).flatten(),
                    agent_usage: send_usage.then_some(usage_json.as_ref()).flatten(),
                    doctor: doctor_json.as_ref(),
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
            // loop also owns remote starts).
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

        if let Some(worker) = &automation_worker {
            if last_automation_tick.elapsed() >= AUTOMATION_TICK {
                last_automation_tick = Instant::now();
                worker.send(AutomationWork::Tick).ok();
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
                    "server rejected this build (426) — updating once {live} live session(s) close or sit idle for {}h (a gated run ends itself once the server has swept its row, EXP-681)",
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
                        "ending session {} ({}) — idle for {}h, a {trigger:?} update is waiting",
                        live.session.session_id,
                        live.branch,
                        live.idle_since.map(|since| since.elapsed().as_secs() / 3600).unwrap_or(0)
                    );
                    live.session.kill();
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
        // remote start online without a restart; removing the last hangs up.
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
                                runtime, &ctx, &device_id, &device_label, &advertised, &inbox_tx,
                                &check_in,
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
    for session in &live {
        session.kill();
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
/// UI can show the machine offline with a reason; an older server without
/// the router must never break the daemon. Returns whether the register
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
            // EXP-749: the ACP-ready subset, sent even when empty (a NULL
            // column means "older build, assume all").
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
            // presence reads online, so say that loudly instead of the
            // best-effort "older server" shrug (which stays for transport
            // errors and genuinely routerless servers).
            match &err {
                api::ApiError::Http { status: 400..=499, .. } => log::error!(
                    "devices.register rejected ({err}) — this machine will not appear in the web UI until a register succeeds (retrying on the heartbeat cadence)"
                ),
                _ => log::warn!("devices.register failed (older server?): {err}"),
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
    inbox: &flume::Sender<RemoteStart>,
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
    let inbox = inbox.clone();
    let on_start: StartSessionFn = Arc::new(move |start| {
        let _ = inbox.send(start);
    });
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
            coding_session_id: session_id.clone(),
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
        on_start,
        on_check_in,
        on_history_request,
        on_history_page,
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

fn reconcile_stale_sessions(ctx: &Ctx) {
    let stale = registry::stale_ids(&ctx.data_dir, &ctx.account.id);
    if stale.is_empty() {
        return;
    }
    let trpc = Arc::clone(&ctx.trpc);
    let data_dir = ctx.data_dir.clone();
    std::thread::spawn(move || {
        for id in stale {
            let result = api::coding_sessions::end(&trpc, &id);
            if registry::end_outcome_resolves(&result) {
                registry::remove(&data_dir, &id);
            } else {
                // Still unresolved — keep it for the next reconcile, but an
                // entry this daemon's pid once owned (exec_self keeps the
                // pid) must not read as a live session (EXP-641).
                registry::mark_ended(&data_dir, &id);
            }
        }
    });
}

// ---------------------------------------------------------------------------
// EXP-1005: the account-rotation wall beat
// ---------------------------------------------------------------------------

/// How often the loop reads its live runs' usage walls. The switch itself
/// waits for the turn slot, so a finer beat would buy nothing.
const ROTATION_BEAT: Duration = Duration::from_secs(5);

/// One live run's wall in the rotation's vocabulary — `None` unless the
/// wall is a rate limit. `account` = the run registry's recorded login
/// (`None` = the ambient one, `system`); a record with no clone (a scratch
/// run), or no record at all, is a run a switch could not resume.
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
        repo_less: record.map_or(true, |record| record.clone.is_none()),
    })
}

/// The log-once key of a hold: its variant, never its timestamps (a
/// cooldown's `until_ms` does not move, but a re-parked wait's does).
fn hold_key(hold: &coding::account_rotation::Hold) -> &'static str {
    use coding::account_rotation::Hold;
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
/// (one HTTPS GET per profile), then switches the run (a resume naming the
/// target, the same path a remote "switch account" takes) or parks it.
/// EXP-1107: ONE read per AGENT per beat, every walled run of that agent
/// decided off it (`plan_beat` / `decide_batch`); a switch that fails or is
/// skipped is undone (`undo_switch`).
struct RotationHost {
    ctx: Arc<Ctx>,
    runtime: Option<Arc<steer::SteerRuntime>>,
    sessions: Sessions,
    personal_key: Option<String>,
    /// The same start claims the inbox takes: a rotation's resume holds
    /// `resume:<id>` like a relay resume frame, so the two never relaunch
    /// one run twice.
    reservations: StartReservations,
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
        let runtime = self.runtime.clone();
        let sessions = Arc::clone(&self.sessions);
        let personal_key = self.personal_key.clone();
        let tracker = Arc::clone(&self.tracker);
        let reservations = self.reservations.clone();
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
                let switched = act_on_rotation(
                    &ctx,
                    runtime.as_ref(),
                    &sessions,
                    personal_key.clone(),
                    &reservations,
                    run,
                    decision,
                );
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
/// before it began (a resume of the run already in flight, or the agent
/// started a turn): the caller undoes the rotation the tracker counted. A
/// wait, a switch made, or a switch that FAILED (no local record, workspace
/// gone, the run did not stop in time, a launch error) is `true` — like the
/// desktop's, a failure keeps its count and the cooldown spaces the retry,
/// rather than a forced probe of every profile every beat for a run that
/// can never resume.
#[allow(clippy::too_many_arguments)]
fn act_on_rotation(
    ctx: &Arc<Ctx>,
    runtime: Option<&Arc<steer::SteerRuntime>>,
    sessions: &Sessions,
    personal_key: Option<String>,
    reservations: &StartReservations,
    run: &coding::account_rotation::WalledRun,
    decision: coding::account_rotation::Decision,
) -> bool {
    match decision {
        coding::account_rotation::Decision::Switch {
            target,
            target_label,
            prompt,
        } => {
            log::info!(
                "account rotation [{}]: switching {} -> {target} ({target_label})",
                run.session_id,
                run.account
            );
            // The server inherits started_reason and the parent from the
            // predecessor.
            // EXP-1158: a LOCAL origin, like the desktop host's — a person's
            // relay switch moves the device's last used login, a rotation hop
            // never does (`coding::prepare`).
            let origin = coding::LaunchOrigin::Local;
            // REV-9: the resume claim a relay resume frame takes. Held by a
            // frame in flight = that resume is already moving the run.
            let _reservation = match reservations.claim(vec![format!("resume:{}", run.session_id)]) {
                Ok(reservation) => reservation,
                Err(clash) => {
                    log::warn!(
                        "account rotation [{}]: switch skipped, a start holding {clash} is in flight",
                        run.session_id
                    );
                    return false;
                }
            };
            match remote_resume_start(
                ctx,
                runtime,
                sessions,
                personal_key,
                origin,
                run.session_id.clone(),
                Some(target),
                Some(prompt),
            ) {
                Ok(()) => true,
                Err(err) if err.is::<SwitchBusy>() => {
                    log::warn!(
                        "account rotation [{}]: switch refused — the agent started a turn; retrying soon",
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
// Remote-start dispatch (steer_wiring's handle_remote_start, headless)
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
fn handle_remote_start(
    ctx: &Ctx,
    runtime: Option<&Arc<steer::SteerRuntime>>,
    sessions: &Sessions,
    personal_key: Option<String>,
    device_id: &str,
    start: RemoteStart,
) {
    // Frame options over settings defaults, capability-masked; plan mode
    // defaults OFF for remote starts (F7 — never park an unattended box at
    // the plan TUI unless the sender opted in).
    let settings = coding::Settings::load(&coding::Settings::default_path(&ctx.data_dir));
    let options = LaunchOptions::remote(
        &settings,
        start.agent.as_deref(),
        start.model.as_deref(),
        start.effort.as_deref(),
        start.ultracode,
        start.plan_mode,
        // EXP-849: the composer's account pick; EXP-792's server picks rode the
        // frame unread until now.
        start.account.as_deref(),
    )
    .with_mcp_servers(start.mcp_server_ids.clone())
    // EXP-981: the composer's claude-only subagent pick; absent leaves this
    // machine's own launch default in place.
    .with_subagent_model(start.subagent_model.as_deref());
    let origin = coding::LaunchOrigin::Relay {
        device_id: device_id.to_string(),
        claimant: ctx.account.id.clone(),
        started_by: start.started_by.clone(),
        // EXP-679: `agent` — another coding session asked for this start, so
        // the run is unattended and its close-out ends it.
        started_reason: start.started_reason.clone(),
    };

    // Success is observed purely via the synced `coding_sessions` row
    // appearing (desktop parity). FEED-63: errors and refusals are logged
    // AND, when the frame named its `startId`, reported back to the server.
    let outcome = match start.subject.clone() {
        RemoteStartSubject::Issue(issue_id) => remote_issue_start(
            ctx, runtime, sessions, personal_key, options, origin, issue_id,
            // EXP-481: honor the remote resume flag — the launcher's marker
            // gate degrades a missing/foreign worktree to a fresh session.
            start.resume,
            start.prompt.clone(),
        ),
        RemoteStartSubject::Batch { issue_ids, team_id, repo } => remote_batch_start(
            ctx, runtime, sessions, personal_key, options, origin, issue_ids, team_id, repo,
            start.prompt.clone(),
        ),
        RemoteStartSubject::Action { action_id, team_id, repo, inputs, .. } => remote_action_start(
            ctx, runtime, sessions, personal_key, options, origin, action_id, team_id, repo, inputs,
            start.prompt.clone(),
        ),
        // EXP-637: the run registry holds everything else (agent, workspace,
        // branch, options), so the frame's launch options are ignored by
        // contract — a resumed run keeps what it recorded. EXP-849: all but
        // ONE — the ACCOUNT, which is how a remote "switch account" reaches
        // this machine (a resume naming a different login).
        RemoteStartSubject::Resume { session_id } => remote_resume_start(
            ctx, runtime, sessions, personal_key, origin, session_id,
            start.account.clone(),
            None,
        ),
    };
    match outcome {
        Ok(()) => {}
        // FEED-63: a deliberate refusal (the one-session-per-issue guard, a
        // disabled launch) stays a warn, as before, and now reaches the
        // requester too.
        Err(err) if err.is::<StartRefused>() => {
            log::warn!("remote start refused ({:?}): {err}", start.subject);
            steer::report_start_failure(&ctx.trpc, start.start_id.as_deref(), &err.to_string());
        }
        Err(err) => {
            log::warn!("remote start failed: {err:#}");
            steer::report_start_failure(
                &ctx.trpc,
                start.start_id.as_deref(),
                &format!("{err:#}"),
            );
        }
    }
}

/// FEED-63: a remote start this machine did not honour, as a typed error so
/// [`handle_remote_start`] can tell a deliberate refusal (reported with its
/// reason, logged at warn like before) from a failure. The `Display` is the
/// reason the requester reads.
#[derive(Debug)]
struct StartRefused(String);

impl std::fmt::Display for StartRefused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for StartRefused {}

/// The one-session-per-issue guards every issue-shaped start takes — this
/// daemon's own live sessions first, then the REV2-24 cross-device probe
/// (desktop parity: one session per issue, wherever it runs). `Some` refuses
/// the start and carries the reason; the probe is best-effort, so an older
/// server without it never blocks.
fn issue_start_blocker(ctx: &Ctx, sessions: &Sessions, issue_id: &str) -> Option<String> {
    issue_start_blocker_except(ctx, sessions, issue_id, None)
}

/// [`issue_start_blocker`], with ONE session exempted: the run a CONTINUATION
/// replaces (EXP-849's account switch). The live row it is about to take over
/// is the same piece of work, so counting it would make a switch impossible
/// while the run it switches is still live — the desktop's
/// `coding_flow::resume_blocker_for` exempts the same chain. Another machine's
/// session on the issue still refuses.
fn issue_start_blocker_except(
    ctx: &Ctx,
    sessions: &Sessions,
    issue_id: &str,
    except: Option<&str>,
) -> Option<String> {
    if let Some(session_id) = issue_run_here(sessions, issue_id) {
        return Some(steer::issue_held_here_reason(&session_id));
    }
    if let Ok(Some(live)) = api::coding_sessions::live_for_issue(&ctx.trpc, issue_id) {
        if except != Some(live.id.as_str()) {
            return Some(steer::issue_held_elsewhere_reason(
                live.device_label.as_deref().unwrap_or("another device"),
                Some(&live.id),
            ));
        }
    }
    None
}

/// EXP-662 — what a remote start's `resume` flag resolves to: the newest
/// still-resumable ISSUE record on this account, or nothing. The flag gates
/// the lookup, so an unchecked box never relaunches a transcript, and a miss
/// degrades to a fresh session seeded with the resume prompt.
fn issue_resume_record(
    data_dir: &Path,
    account_id: &str,
    issue_id: &str,
    start_resume: bool,
) -> Option<coding::run_registry::RunRecord> {
    // FEED-47: only the explicit flag reads the registry; a fresh start on an
    // issue with a recorded run is a NEW run in the reused worktree.
    coding::launcher::recorded_run_for_start(start_resume, || {
        coding::run_registry::latest_for_issue(data_dir, account_id, issue_id)
    })
}

/// EXP-862 — the logins the runs this daemon hosts are using right now, as
/// `(agent, profile id)` pairs (`agent_profile_remove` and, EXP-1137,
/// `agent_profile_sign_out` refuse to pull one out from under a live run).
/// The live row does not carry the account, the run RECORD does
/// (`runs.json`), so the live ids resolve through the registry — ONE load for
/// the whole pass (`run_registry::all`), never one parse per session. Order
/// follows `session_ids`; unknown ids are skipped. EXP-1137: an ambient
/// (account-less) run is a run on the `system` login, not no login — it is
/// what an ambient sign-out must refuse for. Mirrored by the IDE's
/// `device_sync::live_run_accounts`.
fn live_run_accounts(
    data_dir: &Path,
    session_ids: &[String],
) -> Vec<(coding::CodingAgent, String)> {
    let records = coding::run_registry::all(data_dir);
    session_ids
        .iter()
        .filter_map(|id| records.iter().find(|record| record.session_id == *id))
        .map(|record| {
            (
                record.agent,
                coding::profile_id(record.account().as_deref()),
            )
        })
        .collect()
}

#[allow(clippy::too_many_arguments)]
fn remote_issue_start(
    ctx: &Ctx,
    runtime: Option<&Arc<steer::SteerRuntime>>,
    sessions: &Sessions,
    personal_key: Option<String>,
    options: LaunchOptions,
    origin: coding::LaunchOrigin,
    issue_id: String,
    start_resume: bool,
    prompt: Option<String>,
) -> anyhow::Result<()> {
    if let Some(reason) = issue_start_blocker(ctx, sessions, &issue_id) {
        // FEED-63: still a refusal, not a failure; typed so the caller
        // reports the reason to the requester.
        return Err(StartRefused(reason).into());
    }
    let fetched = api::issues::issues_get(&ctx.trpc, &issue_id).context("resolve the issue")?;
    let issue = fetched.issue;
    let mut seeds = HashMap::new();
    seeds.insert(issue.id.clone(), launch::issue_seed(&issue));
    let deps = launch::coding_deps(ctx, seeds, launch::LaunchHost::Daemon, runtime);
    // EXP-662: a recorded run relaunches its EXACT transcript (the recorded
    // agent, workspace and identity pin); only a record-less resume falls
    // through to a fresh session carrying the resume PROMPT.
    let request = match issue_resume_record(&ctx.data_dir, &ctx.account.id, &issue.id, start_resume)
    {
        Some(record) => PrepareRequest::ResumeRun(coding::ResumeRunRequest {
            record,
            device_label: coding::default_device_label(),
            origin,
            model: None,
            effort: None,
            prompt,
            // EXP-849: a remote "switch account" IS a resume naming one. The
            // frame's pick, already normalized (`system`/blank → the ambient
            // login) by [`LaunchOptions::remote`].
            account: options.account.clone(),
        }),
        None => PrepareRequest::Issue(launch::issue_launch_request(
            &issue,
            options,
            origin,
            start_resume,
            prompt,
        )),
    };
    let prepared = coding::prepare(&request, &deps)
        .map_err(|err| anyhow::anyhow!("{err}"))?;
    spawn_prepared(
        ctx, runtime, sessions, personal_key, prepared,
        Some(issue.id), false,
    )
}

#[allow(clippy::too_many_arguments)]
fn remote_batch_start(
    ctx: &Ctx,
    runtime: Option<&Arc<steer::SteerRuntime>>,
    sessions: &Sessions,
    personal_key: Option<String>,
    options: LaunchOptions,
    origin: coding::LaunchOrigin,
    issue_ids: Vec<String>,
    team_id: String,
    repo: steer::StartRepoGroup,
    prompt: Option<String>,
) -> anyhow::Result<()> {
    let mut issues = Vec::new();
    let mut seeds = HashMap::new();
    // EXP-712: the board the batch branch is cut from — the first resolved
    // issue's. `steer.startSession` already refused a batch whose boards
    // disagree on the base branch, so any of them names the same branch.
    let mut batch_board: Option<String> = None;
    for issue_id in issue_ids {
        // Only a genuinely UNKNOWN id is skipped (desktop parity — its sync
        // store may lag too). A transport/auth error must abort the whole
        // batch instead of silently shrinking it to whatever happened to
        // resolve before the network blipped.
        let fetched = match api::issues::issues_get(&ctx.trpc, &issue_id) {
            Ok(fetched) => fetched,
            Err(api::ApiError::Http { status: 404, .. }) => {
                log::info!("batch start: issue {issue_id} skipped (not found)");
                continue;
            }
            Err(err) => {
                anyhow::bail!("batch start: issue {issue_id} failed to resolve ({err}) — aborting");
            }
        };
        if fetched.team_id != team_id {
            anyhow::bail!("batch start: issue {issue_id} is outside the claimed team — aborting");
        }
        if issue_is_coding_here(sessions, &issue_id) {
            anyhow::bail!("batch start: {} is already being coded here — aborting", fetched.issue.identifier);
        }
        // REV2-24 cross-device guard, batch shape (desktop parity: any live
        // session aborts the WHOLE batch). Best-effort on older servers.
        if let Ok(Some(live)) = api::coding_sessions::live_for_issue(&ctx.trpc, &issue_id) {
            anyhow::bail!(
                "batch start: {} has a live session on {} — aborting",
                fetched.issue.identifier,
                live.device_label.as_deref().unwrap_or("another device")
            );
        }
        let issue = fetched.issue;
        if batch_board.is_none() {
            batch_board = issue.board_id.clone();
        }
        seeds.insert(issue.id.clone(), launch::issue_seed(&issue));
        issues.push(coding::BatchIssueSpec {
            issue_id: issue.id.clone(),
            issue_identifier: issue.identifier.clone(),
            title: issue.title.clone(),
            description: issue.description.clone(),
            status: domain::IssueStatus::from_wire(issue.status.as_deref().unwrap_or("")),
        });
    }
    if issues.is_empty() {
        anyhow::bail!("batch start: no launchable issues");
    }
    let request = coding::BatchLaunchRequest {
        batch_id: coding::new_batch_id(),
        team_id,
        board_id: batch_board,
        repo: coding::RepoGroup {
            repository_id: repo.repository_id,
            full_name: repo.full_name,
            default_branch: repo.default_branch,
        },
        issues,
        device_label: coding::default_device_label(),
        origin,
        options,
        prompt,
    };
    let covered: Vec<String> = request.issues.iter().map(|issue| issue.issue_id.clone()).collect();
    let deps = launch::coding_deps(ctx, seeds, launch::LaunchHost::Daemon, runtime);
    let request = PrepareRequest::Batch(request);
    let prepared = coding::prepare(&request, &deps)
        .map_err(|err| anyhow::anyhow!("{err}"))?;
    spawn_prepared_covering(ctx, runtime, sessions, personal_key, prepared, None, covered, false)
}

#[allow(clippy::too_many_arguments)]
fn remote_action_start(
    ctx: &Ctx,
    runtime: Option<&Arc<steer::SteerRuntime>>,
    sessions: &Sessions,
    personal_key: Option<String>,
    options: LaunchOptions,
    origin: coding::LaunchOrigin,
    action_id: String,
    team_id: String,
    repo: Option<steer::StartRepoGroup>,
    inputs: Vec<steer::StartInput>,
    prompt: Option<String>,
) -> anyhow::Result<()> {
    let inputs: Vec<coding::ActionInputValue> = inputs
        .into_iter()
        .map(|input| coding::ActionInputValue {
            label: input.label.clone().unwrap_or_else(|| input.key.clone()),
            input_type: input.input_type.clone().unwrap_or_else(|| "text".to_string()),
            key: input.key,
            value: input.value,
            display: input.display,
        })
        .collect();
    let repo = repo.map(|group| coding::RepoGroup {
        repository_id: group.repository_id,
        full_name: group.full_name,
        default_branch: group.default_branch,
    });
    let request = launch::resolve_action_request(
        ctx,
        &action_id,
        &team_id,
        ActionRepo::Provided(repo),
        inputs,
        options,
        origin,
        // A relay frame is a person pressing Run — never automation-started.
        None,
        None,
        prompt,
    )?;

    // Fix-conflicts branch takeover (desktop `take_over_branch`): a live
    // fix run on the branch refuses; other holders are killed first so the
    // rebase never runs under a live PTY's cwd.
    let mut is_fix_run = false;
    if let coding::ActionRunKind::FixConflicts { branch, .. } = &request.kind {
        is_fix_run = true;
        let holders: Vec<Arc<RunningSession>> = {
            let guard = lock_sessions(sessions);
            if guard
                .iter()
                .any(|live| live.branch == *branch && live.is_fix_run && !live.session.is_done())
            {
                anyhow::bail!("a fix-conflicts run is already working this pull request");
            }
            guard
                .iter()
                .filter(|live| live.branch == *branch && !live.session.is_done())
                .map(|live| Arc::clone(&live.session))
                .collect()
        };
        for holder in &holders {
            holder.kill();
        }
        for holder in &holders {
            let _ = holder.wait_timeout(Duration::from_secs(5));
        }
    }

    let deps = launch::coding_deps(ctx, HashMap::new(), launch::LaunchHost::Daemon, runtime);
    let request = PrepareRequest::Action(request);
    let prepared = coding::prepare(&request, &deps)
        .map_err(|err| anyhow::anyhow!("{err}"))?;
    spawn_prepared(ctx, runtime, sessions, personal_key, prepared, None, is_fix_run)
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

/// EXP-849 — end the live run an account switch is taking over, so the resume
/// can re-enter it under the other login: the agent cannot be in two processes
/// in one worktree.
///
/// The engine's own end sequence runs, which flips the row to
/// `ended_by: client` (never `merge`/`system`: nothing merged, and a person
/// asked for this). A run that is MID-TURN is refused instead — the switch is
/// a launch-time decision, and the turn slot is its authority. A run this
/// daemon is not hosting (already ended, or never here) is a no-op: the resume
/// below is then an ordinary one.
fn end_for_account_switch(sessions: &Sessions, session_id: &str) -> anyhow::Result<()> {
    let live = lock_sessions(sessions)
        .iter()
        .find(|live| live.session.session_id == session_id && !live.session.is_done())
        .map(|live| live.session.clone());
    let Some(live) = live else {
        return Ok(());
    };
    if !live.is_idle() {
        return Err(SwitchBusy { session_id: session_id.to_string() }.into());
    }
    log::info!("account switch: ending live session {session_id} before the resume");
    // FEED-68: this end is half of a resume — an agent parent must not read
    // it as "ended without a report" and resume the run a second time.
    coding::mark_resuming(session_id);
    live.kill();
    // ~10s: an engine teardown is sub-second. Past that the resume is refused
    // rather than launched into a worktree something may still hold.
    if live
        .wait_timeout(std::time::Duration::from_secs(10))
        .is_none()
    {
        coding::unmark_resuming(session_id);
        anyhow::bail!("account switch for {session_id} refused — the run did not stop in time");
    }
    Ok(())
}

/// EXP-637 — RESUME an ended run of ANY kind out of the local run registry
/// (EXP-662 added the issue and batch shapes). A record this daemon never
/// wrote (or whose workspace is gone) is a hard refusal, logged: the requester
/// sees no new session row, exactly like every other refused start.
#[allow(clippy::too_many_arguments)]
fn remote_resume_start(
    ctx: &Ctx,
    runtime: Option<&Arc<steer::SteerRuntime>>,
    sessions: &Sessions,
    personal_key: Option<String>,
    origin: coding::LaunchOrigin,
    session_id: String,
    account: Option<String>,
    prompt: Option<String>,
) -> anyhow::Result<()> {
    let Some(record) = coding::run_registry::get(&ctx.data_dir, &session_id) else {
        anyhow::bail!(
            "no local record for run {session_id}: it ran on another machine, was a repo-less \
run (purged when it ends), or was removed by this machine's session history setting"
        );
    };
    if !record.resumable() {
        anyhow::bail!("run {session_id}'s workspace is gone and cannot be re-created");
    }
    // EXP-849: a resume that NAMES an account is a mid-run "switch account" —
    // the server lets such a frame ride a run that is still LIVE, and THIS
    // machine is the one that ends it. Mid-turn it is refused instead: a switch
    // then would truncate exactly the output the requester is watching.
    let switching = account.as_deref().is_some_and(|id| !id.trim().is_empty());
    if switching {
        end_for_account_switch(sessions, &session_id)?;
    }
    // The run being continued never blocks its own continuation.
    let except = switching.then(|| session_id.clone());
    // EXP-662: an issue/batch record resumes as a SESSION on those issues, so
    // it takes the same one-session-per-issue guards a fresh start does.
    let mut seeds = HashMap::new();
    match record.kind {
        coding::run_registry::RunKind::Issue => {
            if let Some(issue_id) = record.issue_id.clone() {
                if let Some(reason) =
                    issue_start_blocker_except(ctx, sessions, &issue_id, except.as_deref())
                {
                    anyhow::bail!(reason);
                }
                // Best-effort seed: it only feeds the FALLBACK prompt (the
                // issue's title) for a record whose native transcript is
                // gone. A failed fetch still resumes.
                if let Ok(fetched) = api::issues::issues_get(&ctx.trpc, &issue_id) {
                    seeds.insert(issue_id, launch::issue_seed(&fetched.issue));
                }
            }
        }
        coding::run_registry::RunKind::Batch => {
            for issue in &record.issues {
                if let Some(reason) =
                    issue_start_blocker_except(ctx, sessions, &issue.issue_id, except.as_deref())
                {
                    anyhow::bail!(reason);
                }
            }
        }
        _ => {}
    }
    // The LiveSession/steer room is issue-shaped for an issue record, exactly
    // like the fresh start it continues.
    let issue_id = record.issue_id.clone();
    let request = coding::ResumeRunRequest {
        record,
        device_label: coding::default_device_label(),
        origin,
        model: None,
        effort: None,
        // The server never sends a prompt on a resume frame (EXP-825);
        // EXP-1005's rotation passes its switch note (the launcher appends
        // the continue prompt after it).
        prompt,
        // EXP-849: the frame's account pick — absent keeps the recorded login.
        account,
    };
    let deps = launch::coding_deps(ctx, seeds, launch::LaunchHost::Daemon, runtime);
    let request = PrepareRequest::ResumeRun(request);
    let prepared = coding::prepare(&request, &deps)
        .map_err(|err| anyhow::anyhow!("{err}"))?;
    spawn_prepared(ctx, runtime, sessions, personal_key, prepared, issue_id, false)
}

#[allow(clippy::too_many_arguments)]
fn spawn_prepared(
    ctx: &Ctx,
    runtime: Option<&Arc<steer::SteerRuntime>>,
    sessions: &Sessions,
    personal_key: Option<String>,
    prepared: Prepared,
    issue_id: Option<String>,
    is_fix_run: bool,
) -> anyhow::Result<()> {
    spawn_prepared_covering(ctx, runtime, sessions, personal_key, prepared, issue_id, Vec::new(), is_fix_run)
}

/// [`spawn_prepared`] for a BATCH run, which records the issues it covers.
#[allow(clippy::too_many_arguments)]
fn spawn_prepared_covering(
    ctx: &Ctx,
    runtime: Option<&Arc<steer::SteerRuntime>>,
    sessions: &Sessions,
    personal_key: Option<String>,
    prepared: Prepared,
    issue_id: Option<String>,
    batch_issue_ids: Vec<String>,
    is_fix_run: bool,
) -> anyhow::Result<()> {
    let prepared = match prepared {
        Prepared::Ready(prepared) => prepared,
        // FEED-63: typed, so a remote start reports the refusal back.
        Prepared::Disabled(reason) => {
            return Err(StartRefused(steer::disabled_launch_reason(&reason.message())).into());
        }
    };
    let env = LaunchEnv {
        ctx,
        runtime,
        personal_key,
        rotation_host: true,
    };
    // EXP-530: an action run's own id — the automation host defers while it
    // is live.
    let action_id = prepared.action_id.clone();
    // EXP-637: snapshotted before the launch consumes the prepared value.
    let cleanup = prepared.run_cleanup.clone();
    let session = Arc::new(session_host::launch(&env, prepared, issue_id.clone())?);
    log::info!(
        "session {} started ({}, branch {})",
        session.session_id,
        session.issue_identifier,
        session.branch
    );
    register_session(
        sessions,
        LiveSession {
            issue_id,
            batch_issue_ids,
            action_id,
            branch: session.branch.clone(),
            is_fix_run,
            cleanup,
            session,
            idle_since: None,
        },
    );
    Ok(())
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
        // machine's permissions and start the cua worker now, not in the
        // middle of a run; off = stop it (its agent cursor goes with it).
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
            // Offline / older server: queue the retry (next heartbeat's
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
            let profile = command.payload["profileId"].as_str().unwrap_or("system");
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
        // server-wide — and nothing leaves the machine. EXP-1137: the ambient
        // login is signed out and hidden instead. An account a live run here
        // is using is refused.
        //
        // EXP-1137 — "sign out": the same guard, and the login's credential
        // goes the agent's own way while its row stays.
        //
        // Both raise `doctor_soon`: the ambient login's row in the cached
        // report is stale the moment it signs out, and the advertisement
        // (runnable agents) follows the fresh check on the next loop turn.
        kind @ ("agent_profile_remove" | "agent_profile_sign_out") => {
            let agent = command.payload["agent"].as_str().unwrap_or_default();
            let profile = command.payload["profileId"].as_str().unwrap_or("system");
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
        // FEED-36: the web's "Update now" — end every live session (the
        // owner confirmed it; repo-backed runs stay resumable) and let the
        // loop apply the pending update as soon as they close.
        "update_now" => {
            let live: Vec<(String, String)> = lock_sessions(sessions)
                .iter()
                .filter(|live| !live.session.is_done())
                .map(|live| (live.session.session_id.clone(), live.branch.clone()))
                .collect();
            for (session_id, branch) in &live {
                log::info!("update now (web): ending session {session_id} ({branch})");
            }
            for live_session in lock_sessions(sessions).iter() {
                live_session.session.kill();
            }
            slots.update_now.store(true, Ordering::SeqCst);
            if live.is_empty() {
                (true, "Restarting on the new version.".to_string())
            } else {
                (
                    true,
                    format!(
                        "Ending {} live session(s); the daemon restarts on the new version once they close.",
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
// EXP-530: the trigger host — this daemon's own Electric pipeline (a shape
// subset in `sync-cli.sqlite`), ONE delta-drain thread, and the serialized
// automations worker. The worker evaluates the triggers bound to
// THIS device (`Ctx::device_id`) through the shared `coding::automations`
// engine and self-starts the fired action runs; the sessions it spawns join
// the normal `Sessions` vec, so heartbeat/parked-update/quit-sweep cover them
// like any other run.
// ---------------------------------------------------------------------------

/// What the trigger host syncs: the actions and the `triggers` they carry
/// (`actions` — SLOP-2 folded the old `automations` shape into it), the
/// event feed (`issue_events`), and the rows the prompt lines and board
/// filters read (`issues`, `boards`, `labels`, `issue_statuses`).
/// Deliberately NOT the desktop's 21 — a headless daemon has no views to
/// hydrate.
const AUTOMATION_SHAPES: &[&str] = &[
    "actions",
    "issues",
    "issue_events",
    "boards",
    "labels",
    "issue_statuses",
];

/// The automation self-tick. Event triggers ride the delta drain (they fire
/// within a second of the row landing); this beat is what makes SCHEDULES
/// fire at all, and it is the catch-up pass after a sleep/offline stretch
/// where no delta ever arrives.
const AUTOMATION_TICK: Duration = Duration::from_secs(30);

/// Open the daemon's shape store and start its pipeline. `None` (logged, not
/// fatal) leaves automations dormant while every other daemon duty — remote
/// starts, heartbeat, worktree commands — keeps running.
fn start_automation_sync(ctx: &Ctx, gated: &Arc<AtomicBool>) -> Option<Arc<sync::SyncManager>> {
    // NEVER `sync-v2.sqlite`: that file belongs to the desktop GUI, and two
    // pipelines writing one store would fight over shape offsets.
    let db_path = ctx
        .data_dir
        .join("accounts")
        .join(&ctx.account.id)
        .join("sync-cli.sqlite");
    if let Some(parent) = db_path.parent() {
        if let Err(err) = std::fs::create_dir_all(parent) {
            log::warn!("automations: sync store dir ({err}) — automations disabled");
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
        shapes: Some(AUTOMATION_SHAPES),
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
            log::warn!("automations: sync store failed to open ({err}) — automations disabled");
            None
        }
    }
}

/// Why the worker woke up. Every variant runs the same full re-evaluation;
/// they differ only in whether the (expensive) `issue_events` snapshot has to
/// be re-read — EXP-562: a beat that nothing eventful scheduled reuses the
/// cached rows instead of hydrating the table again.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AutomationWork {
    /// A synced `issue_events` batch landed — the cache is stale.
    EventsChanged,
    /// A synced `actions` batch landed — a trigger was authored, edited or
    /// toggled (they ride `actions.triggers`). Re-decide, but the events
    /// snapshot is untouched.
    ActionsChanged,
    /// The 1s loop's [`AUTOMATION_TICK`] beat.
    Tick,
}

/// The single `deltas()` consumer (flume is MPMC — cloned receivers STEAL,
/// so exactly one place may drain). Applied batches for the two shapes an
/// automation can turn on nudge the worker; everything else is ignored.
fn spawn_delta_drain(manager: &Arc<sync::SyncManager>, worker: flume::Sender<AutomationWork>) {
    let deltas = manager.deltas();
    let spawned = std::thread::Builder::new()
        .name("exp-automations".to_string())
        .spawn(move || {
            while let Ok(delta) = deltas.recv() {
                let sync::ShapeDelta::Applied {
                    shape,
                    keys,
                    full_replace,
                    ..
                } = delta
                else {
                    continue;
                };
                let work = match shape {
                    "issue_events" => AutomationWork::EventsChanged,
                    "actions" => AutomationWork::ActionsChanged,
                    _ => continue,
                };
                // A pure `up-to-date` heartbeat changed nothing.
                if keys.is_empty() && !full_replace {
                    continue;
                }
                // A closed channel is the daemon shutting down.
                if worker.send(work).is_err() {
                    break;
                }
            }
        });
    if let Err(err) = spawned {
        log::warn!("automations: delta drain failed to spawn: {err}");
    }
}

/// Everything the worker thread owns.
struct AutomationHost {
    ctx: Arc<Ctx>,
    runtime: Option<Arc<steer::SteerRuntime>>,
    sessions: Sessions,
    reservations: StartReservations,
    personal_key: Option<String>,
    sync: Arc<sync::SyncManager>,
    device_id: String,
    /// EXP-562: the last hydrated event snapshot, reused by every beat no
    /// `issue_events` batch woke up. `None` = nothing cached (first beat, a
    /// failed read, or a device with no event trigger at all).
    event_cache: Option<Vec<coding::automations::EventRow>>,
}

/// The `spawn_device_worker` idiom: unbounded flume + ONE thread, so two
/// beats never evaluate (or write state) concurrently.
fn spawn_automation_worker(mut host: AutomationHost) -> flume::Sender<AutomationWork> {
    let (tx, rx) = flume::unbounded::<AutomationWork>();
    std::thread::Builder::new()
        .name("exp-automate".to_string())
        .spawn(move || {
            while let Ok(first) = rx.recv() {
                // Coalesce a burst (a refetch is thousands of rows in a
                // handful of batches) into ONE evaluation pass.
                let mut batch = vec![first];
                while let Ok(more) = rx.try_recv() {
                    batch.push(more);
                }
                host.beat(rescan_events(&batch));
            }
        })
        .expect("spawn the automations worker");
    tx
}

/// Whether a coalesced burst invalidates the event snapshot: ONE
/// `issue_events` batch anywhere in it does, a pile of ticks and trigger
/// edits does not.
fn rescan_events(batch: &[AutomationWork]) -> bool {
    batch
        .iter()
        .any(|work| matches!(work, AutomationWork::EventsChanged))
}

/// One trigger bound to this device, plus what the LAUNCH needs beyond
/// the engine's view of it.
#[derive(Clone, Debug)]
struct AutomationAction {
    triggered: coding::automations::TriggeredAutomation,
    team_id: String,
    /// The target action's display name — the log line's handle.
    name: String,
    /// The trigger's pinned agent/account/model/effort; every `None`
    /// falls back to this machine's launch defaults.
    agent: Option<String>,
    /// EXP-995: the agent profile the run spends (belongs to `agent`).
    account: Option<String>,
    model: Option<String>,
    effort: Option<String>,
}

impl AutomationHost {
    /// One evaluation pass. Every failure mode degrades to "do nothing this
    /// beat" — the next nudge or tick retries from fresh state.
    ///
    /// `rescan` is the coalesced burst's verdict on the event snapshot
    /// (EXP-562): false reuses [`Self::event_cache`], so the common
    /// nothing-happened beat costs one settings read instead of hydrating
    /// every synced `issue_events` row plus a point-read per issue.
    fn beat(&mut self, rescan: bool) {
        let Some(store) = self.sync.store(&self.ctx.account.id) else {
            return;
        };
        // EXP-1102: the automation state's one move out of settings.json,
        // once per process.
        static MIGRATED: AtomicBool = AtomicBool::new(false);
        if !MIGRATED.swap(true, Ordering::SeqCst) {
            let settings_path = coding::Settings::default_path(&self.ctx.data_dir);
            if self.automation_store().migrate_legacy(&settings_path) {
                log::info!("moved the automation state out of settings.json");
            }
        }
        let action_rows = read_shape_rows::<domain::rows::ActionRow>(&store, "actions");
        let actions = triggered_actions(&action_rows, &self.device_id);
        if actions.is_empty() {
            return;
        }
        let settings_path = coding::Settings::default_path(&self.ctx.data_dir);
        let mut states = coding::automations::read_states(&self.automation_store());
        let now_local = chrono::Local::now();
        let now_ms = now_local.timestamp_millis();
        let live = live_action_ids(&self.sessions);
        let triggers: Vec<coding::automations::TriggeredAutomation> = actions
            .iter()
            .map(|action| action.triggered.clone())
            .collect();

        if !coding::automations::needs_events(&triggers) {
            // Schedule-only device: never touch the events table, and drop
            // whatever a previously-enabled event trigger left cached.
            self.event_cache = None;
        } else if rescan || self.event_cache.is_none() {
            // A failed read is NOT cached — this beat evaluates with no
            // events (an event trigger simply decides nothing) and the next
            // one retries the scan.
            self.event_cache = read_event_rows(&store, now_ms);
        } else if let Some(cached) = self.event_cache.as_mut() {
            patch_missing_boards(&store, cached);
        }
        let events: &[coding::automations::EventRow] = self.event_cache.as_deref().unwrap_or(&[]);

        let decisions = coding::automations::evaluate(&coding::automations::EvalInput {
            automations: &triggers,
            states: &states,
            events,
            live_action_ids: &live,
            now_ms,
            now_local,
        });

        // Re-seeds are pure bookkeeping (a new or edited trigger anchoring
        // itself) — persist the whole batch in ONE write.
        let mut reseeded = false;
        for decision in &decisions {
            if let coding::automations::Decision::Reseed {
                automation_id,
                new_state,
            } = decision
            {
                states.insert(automation_id.clone(), new_state.clone());
                reseeded = true;
            }
        }
        if reseeded {
            self.persist(&settings_path, &states);
        }

        for decision in decisions {
            let coding::automations::Decision::Fire {
                automation_id,
                firing,
                new_state,
            } = decision
            else {
                continue;
            };
            let Some(action) = actions
                .iter()
                .find(|candidate| candidate.triggered.automation_id == automation_id)
            else {
                continue;
            };
            let action_id = action.triggered.action_id.clone();
            // The key remote action starts hold too (REV-9): a fire racing an
            // in-flight start of the SAME action is dropped WITHOUT a state
            // write, so the next beat re-decides it.
            let reservation = match self.reservations.claim(vec![format!("action:{action_id}")]) {
                Ok(reservation) => reservation,
                Err(clash) => {
                    log::info!(
                        "automation for {action_id} skipped — a start holding {clash} is already in flight"
                    );
                    continue;
                }
            };
            // Watermark-at-launch-START (the firing protocol): persist before
            // launching, so a crash mid-launch drops the run instead of
            // re-firing the same events forever.
            states.insert(automation_id.clone(), new_state.clone());
            if !self.persist(&settings_path, &states) {
                continue;
            }
            let Some(note) = self.trigger_note(&store, action, &firing) else {
                continue;
            };
            log::info!(
                "automation {automation_id} firing action {} ({action_id}) — {}",
                action.name,
                note.started_reason()
            );
            let launched = match self.start(action, note) {
                Ok(launched) => launched,
                Err(err) => {
                    log::warn!("automation {automation_id} failed: {err:#}");
                    false
                }
            };
            drop(reservation);
            if !launched {
                // Poison-pill backoff: the events are already consumed (a
                // trigger that cannot prepare must not hot-loop the same
                // batch), so only the cooldown moves.
                let mut backed_off = new_state;
                backed_off.cooldown_until = Some(
                    backed_off.cooldown_until.unwrap_or(now_ms)
                        + coding::automations::PREPARE_FAILURE_BACKOFF_MS,
                );
                log::warn!("automation {automation_id} backing off after a failed prepare");
                states.insert(automation_id, backed_off);
                self.persist(&settings_path, &states);
            }
        }
    }

    fn persist(
        &self,
        settings_path: &Path,
        states: &HashMap<String, coding::automations::AutomationState>,
    ) -> bool {
        let _ = settings_path;
        match coding::automations::write_states(&self.automation_store(), states) {
            Ok(()) => true,
            Err(err) => {
                // Without a durable watermark a launch could re-fire forever
                // — skip the fire rather than risk that.
                log::warn!("automations: persisting state failed ({err}) — skipping");
                false
            }
        }
    }

    /// The prompt's `## Trigger` section: a schedule carries the shared
    /// sentence, an event the per-issue lines rendered from synced rows.
    fn trigger_note(
        &self,
        store: &sync::store::ShapeStore,
        action: &AutomationAction,
        firing: &coding::automations::Firing,
    ) -> Option<coding::TriggerNote> {
        match firing {
            coding::automations::Firing::Schedule { .. } => {
                match &action.triggered.trigger.kind {
                    coding::automations::TriggerKind::Schedule(schedule) => {
                        Some(coding::TriggerNote {
                            kind: coding::TriggerNoteKind::Schedule {
                                phrase: coding::automations::schedule_phrase(schedule),
                            },
                        })
                    }
                    // Unreachable (the engine only schedule-fires a schedule).
                    _ => None,
                }
            }
            coding::automations::Firing::Event { matches } => {
                let lookups = EventLookups::for_matches(store, matches);
                let lines: Vec<String> = matches
                    .iter()
                    .take(coding::automations::TRIGGER_PROMPT_MAX_LINES)
                    .filter_map(|row| event_line(row, &lookups))
                    .collect();
                Some(coding::TriggerNote {
                    kind: coding::TriggerNoteKind::Event {
                        lines,
                        omitted: matches
                            .len()
                            .saturating_sub(coding::automations::TRIGGER_PROMPT_MAX_LINES),
                    },
                })
            }
        }
    }

    /// The self-fired twin of [`remote_action_start`], minus the steer-frame
    /// mapping: the device's OWN launch defaults, the action's own repo
    /// binding, no inputs (an automation has nobody to prompt),
    /// `LaunchOrigin::Local`. Returns whether a session actually spawned — a
    /// doctor-disabled launcher counts as a FAILED prepare, so the caller
    /// backs the trigger off instead of retrying every beat.
    fn start(&self, action: &AutomationAction, note: coding::TriggerNote) -> anyhow::Result<bool> {
        let settings = coding::Settings::load(&coding::Settings::default_path(&self.ctx.data_dir));
        // EXP-583: the automation's OWN pins win; every unpinned field falls
        // back to this machine's launch defaults. Plan mode is forced off
        // (F7 — an unattended run must never park at the plan-approval TUI).
        let options = coding::automations::launch_options(
            &settings,
            action.agent.as_deref(),
            action.model.as_deref(),
            action.effort.as_deref(),
            action.account.as_deref(),
        );
        let request = launch::resolve_action_request(
            &self.ctx,
            &action.triggered.action_id,
            &action.team_id,
            ActionRepo::Resolve,
            Vec::new(),
            options,
            coding::LaunchOrigin::Local,
            Some(note),
            Some(action.triggered.automation_id.clone()),
            // An automation fires with no composer text.
            None,
        )?;
        let deps = launch::coding_deps(
            &self.ctx,
            HashMap::new(),
            launch::LaunchHost::Daemon,
            self.runtime.as_ref(),
        );
        let request = PrepareRequest::Action(request);
        let prepared = coding::prepare(&request, &deps)
            .map_err(|err| anyhow::anyhow!("{err}"))?;
        if let Prepared::Disabled(reason) = &prepared {
            log::warn!("automation run refused: {}", reason.message());
            return Ok(false);
        }
        spawn_prepared(
            &self.ctx,
            self.runtime.as_ref(),
            &self.sessions,
            self.personal_key.clone(),
            prepared,
            None,
            false,
        )?;
        Ok(true)
    }

    /// EXP-1102: the automation state's own store (`{data_dir}/automations/
    /// <device_id>.json`).
    fn automation_store(&self) -> coding::automations::AutomationStore {
        coding::automations::AutomationStore::open(&self.ctx.data_dir, &self.device_id)
    }
}

/// The triggers this device evaluates, over every synced action
/// ([`coding::automations::bound_triggers`] — the ONE resolution the GUI host
/// shares, so the two agree on what counts as an edit and never re-seed each
/// other's state).
fn triggered_actions(actions: &[domain::rows::ActionRow], device_id: &str) -> Vec<AutomationAction> {
    coding::automations::bound_triggers(actions.iter(), device_id)
        .into_iter()
        .map(|bound| AutomationAction {
            team_id: bound.triggered.team_id.clone(),
            triggered: bound.triggered,
            // The synced row is only the display name here; the launch
            // re-fetches the action fresh (synced rows carry no body).
            name: bound.action_name,
            agent: bound.pins.agent,
            account: bound.pins.account,
            model: bound.pins.model,
            effort: bound.pins.effort,
        })
        .collect()
}

/// Hydrate a whole shape table into `domain::rows` structs (§5.5 tolerant
/// decode — an undecodable row is dropped, never fatal).
fn read_shape_rows<T: serde::de::DeserializeOwned>(
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
            log::debug!("automations: reading the {shape} table failed: {err}");
            Vec::new()
        }
    }
}

/// How far BELOW the catch-up cutoff [`event_scan_bound`] aims. The SQL
/// pre-filter compares raw text, so the margin absorbs everything text order
/// cannot model — device/server clock skew and a sub-second or offset-suffix
/// render difference — and keeps the scan a strict superset of the window
/// the Rust filter then applies exactly.
const EVENT_SCAN_MARGIN_MS: i64 = 3_600_000;

/// The `created_at >= ?` bound for the event scan: the catch-up cutoff minus
/// [`EVENT_SCAN_MARGIN_MS`], rendered UTC in the space form Electric emits
/// (`2026-08-18 06:00:00…`) so a byte compare orders it correctly against
/// both wire forms — see [`sync::store::ShapeStore::read_where_ge`]. An
/// unrepresentable instant degrades to the empty bound: a full scan, i.e. the
/// old behaviour, never a missed row.
fn event_scan_bound(now_ms: i64) -> String {
    let floor_ms = now_ms - domain::contract::AUTOMATION_EVENT_CATCHUP_MS - EVENT_SCAN_MARGIN_MS;
    chrono::DateTime::from_timestamp_millis(floor_ms)
        .map(|at| at.format("%Y-%m-%d %H:%M:%S").to_string())
        .unwrap_or_default()
}

/// The candidate event rows: inside the contract catch-up window, with
/// `board_id` pre-joined from the issue (the `issue_events` shape carries
/// none, and the engine's board filter needs it — a missing datum
/// conservatively FAILS a non-empty filter).
///
/// SQL narrows the table to the window first (EXP-562 — hydrating every
/// synced event every beat was the cost); the exact `created_at_ms < cutoff`
/// filter still runs here, because the text bound is only a superset.
/// `None` means the read FAILED — the caller evaluates with no events and
/// caches nothing, so the next beat retries.
fn read_event_rows(
    store: &sync::store::ShapeStore,
    now_ms: i64,
) -> Option<Vec<coding::automations::EventRow>> {
    let spec = sync::shapes::shape_by_name("issue_events")?;
    let raw = match store.read_where_ge(spec, "created_at", &event_scan_bound(now_ms)) {
        Ok(raw) => raw,
        Err(err) => {
            log::debug!("automations: scanning the issue_events table failed: {err}");
            return None;
        }
    };
    let cutoff = now_ms - domain::contract::AUTOMATION_EVENT_CATCHUP_MS;
    let mut boards: HashMap<String, Option<String>> = HashMap::new();
    Some(
        raw.into_iter()
            // §5.5 tolerant decode — an undecodable row is dropped, never fatal.
            .filter_map(|row| {
                serde_json::from_value::<domain::rows::IssueEvent>(serde_json::Value::Object(row))
                    .ok()
            })
            .filter_map(|row| {
                let created_at_ms = row.created_at.as_deref().and_then(parse_epoch_ms)?;
                if created_at_ms < cutoff {
                    return None;
                }
                let board_id = boards
                    .entry(row.issue_id.clone())
                    .or_insert_with(|| issue_field(store, &row.issue_id, "board_id"))
                    .clone();
                Some(coding::automations::EventRow {
                    id: row.id,
                    issue_id: row.issue_id,
                    // The engine fences matching to the action's own team — a
                    // missing value conservatively never matches.
                    team_id: row.team_id,
                    created_at_ms,
                    kind: row.kind.unwrap_or_default(),
                    payload: row.payload,
                    board_id,
                })
            })
            .collect(),
    )
}

/// Re-join the `board_id` of CACHED rows that still have none: `issue_events`
/// routinely syncs ahead of its issue, and an unknown board conservatively
/// FAILS a board filter — so a miss frozen into the cache would silently
/// never match, even once the issue lands. Only the misses are point-read
/// (deduped per issue), so a cached beat stays O(unresolved issues).
fn patch_missing_boards(
    store: &sync::store::ShapeStore,
    rows: &mut [coding::automations::EventRow],
) {
    let mut resolved: HashMap<String, Option<String>> = HashMap::new();
    for row in rows.iter_mut().filter(|row| row.board_id.is_none()) {
        row.board_id = resolved
            .entry(row.issue_id.clone())
            .or_insert_with(|| issue_field(store, &row.issue_id, "board_id"))
            .clone();
    }
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

/// Tolerant ISO-8601 → MS epoch (the watermark scale). Electric/Postgres
/// emit both the RFC 3339 form and the `2026-07-03 10:00:00+00` space form —
/// the twin of the ui crate's `comments::parse_epoch`, in milliseconds.
fn parse_epoch_ms(raw: &str) -> Option<i64> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Ok(parsed) = chrono::DateTime::parse_from_rfc3339(trimmed) {
        return Some(parsed.timestamp_millis());
    }
    let t_form = trimmed.replacen(' ', "T", 1);
    for candidate in [t_form.clone(), format!("{t_form}:00"), format!("{t_form}Z")] {
        if let Ok(parsed) = chrono::DateTime::parse_from_rfc3339(&candidate) {
            return Some(parsed.timestamp_millis());
        }
    }
    None
}

/// Actions with a STILL-RUNNING local session — the engine defers those (one
/// run of an action at a time on one machine).
fn live_action_ids(sessions: &Sessions) -> HashSet<String> {
    lock_sessions(sessions)
        .iter()
        .filter(|live| !live.session.is_done())
        .filter_map(|live| live.action_id.clone())
        .collect()
}

/// The names an event prompt line renders (host-filled from the sync store;
/// tests pass literals).
#[derive(Debug, Default)]
struct EventLookups {
    /// issue id → (identifier, title)
    issues: HashMap<String, (String, String)>,
    /// label id → name
    labels: HashMap<String, String>,
    /// `issue_statuses` id → name
    statuses: HashMap<String, String>,
}

impl EventLookups {
    /// Point-read the matched issues (capped like the prompt itself) and
    /// take the two small team-wide tables whole.
    fn for_matches(
        store: &sync::store::ShapeStore,
        matches: &[coding::automations::EventRow],
    ) -> Self {
        let mut issues: HashMap<String, (String, String)> = HashMap::new();
        for row in matches
            .iter()
            .take(coding::automations::TRIGGER_PROMPT_MAX_LINES)
        {
            if issues.contains_key(&row.issue_id) {
                continue;
            }
            let Some(identifier) = issue_field(store, &row.issue_id, "identifier") else {
                continue;
            };
            let title = issue_field(store, &row.issue_id, "title").unwrap_or_default();
            issues.insert(row.issue_id.clone(), (identifier, title));
        }
        Self {
            issues,
            labels: read_shape_rows::<domain::rows::Label>(store, "labels")
                .into_iter()
                .map(|label| (label.id, label.name))
                .collect(),
            statuses: read_shape_rows::<domain::rows::IssueStatusRow>(store, "issue_statuses")
                .into_iter()
                .map(|status| (status.id, status.name))
                .collect(),
        }
    }
}

/// One prompt line for a matched event: `EXP-42 "Title" <what changed>`.
/// `None` when the issue is not synced locally — a line naming no issue
/// tells the agent nothing (the run still starts; the watermark already
/// accounted for the row).
fn event_line(row: &coding::automations::EventRow, lookups: &EventLookups) -> Option<String> {
    let (identifier, title) = lookups.issues.get(&row.issue_id)?;
    let payload = |key: &str| {
        row.payload
            .as_ref()
            .and_then(|payload| payload.get(key))
            .and_then(serde_json::Value::as_str)
            .map(str::to_string)
    };
    let tail = match row.kind.as_str() {
        "status_changed" => format!(
            "status {} → {}",
            status_name(lookups, payload("fromName"), payload("fromStatusId"), payload("from")),
            status_name(lookups, payload("toName"), payload("toStatusId"), payload("to")),
        ),
        "priority_changed" => format!(
            "priority {} → {}",
            or_none(payload("from")),
            or_none(payload("to"))
        ),
        "created" => format!("created ({})", or_none(payload("priority"))),
        "label_added" => {
            let id = payload("labelId").unwrap_or_default();
            let name = lookups
                .labels
                .get(&id)
                .cloned()
                .filter(|name| !name.is_empty())
                .unwrap_or_else(|| if id.is_empty() { "a label".to_string() } else { id });
            format!("label {name} added")
        }
        "assignee_changed" => "assignee changed".to_string(),
        "pr_opened" => "pull request opened".to_string(),
        "pr_merged" => "pull request merged".to_string(),
        // Only the 7 contract kinds can match — a future one still reads.
        other => other.replace('_', " "),
    };
    Some(format!("{identifier} \"{title}\" {tail}"))
}

/// A status side's display name: the payload's own snapshot (EXP-314 writes
/// `fromName`/`toName`), else the team's synced row, else the anchor enum
/// munged (`in_progress` → `in progress`). The chain never fails.
fn status_name(
    lookups: &EventLookups,
    name: Option<String>,
    status_id: Option<String>,
    anchor: Option<String>,
) -> String {
    if let Some(name) = name.filter(|name| !name.is_empty()) {
        return name;
    }
    if let Some(name) = status_id
        .and_then(|id| lookups.statuses.get(&id).cloned())
        .filter(|name| !name.is_empty())
    {
        return name;
    }
    match anchor.filter(|anchor| !anchor.is_empty()) {
        Some(anchor) => anchor.replace('_', " "),
        None => "none".to_string(),
    }
}

/// A cleared/absent priority reads `none` (the web `priorityLabel` shape).
fn or_none(value: Option<String>) -> String {
    value
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "none".to_string())
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

    /// FEED-47/57: a batch run covering the issue holds it like the issue's
    /// own run; a batch over other issues does not.
    #[test]
    fn a_batch_run_covering_the_issue_holds_it() {
        let batch = vec!["i-1".to_string(), "i-2".to_string()];
        assert!(covers_issue(Some("i-1"), &[], "i-1"));
        assert!(covers_issue(None, &batch, "i-2"));
        assert!(!covers_issue(None, &batch, "i-3"));
        assert!(!covers_issue(Some("i-9"), &[], "i-1"));
    }

    /// FEED-47: a fresh remote start never reads the run registry, so a
    /// recorded run on the issue cannot turn it into a resume.
    #[test]
    fn a_fresh_issue_start_resolves_no_recorded_run() {
        let dir = std::env::temp_dir().join(format!("exp-daemon-feed47-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        assert!(issue_resume_record(&dir, "acct", "issue-1", false).is_none());
        let _ = std::fs::remove_dir_all(&dir);
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

    // -----------------------------------------------------------------------
    // EXP-662: the remote `resume` flag resolves through the run registry
    // -----------------------------------------------------------------------

    fn issue_record(cwd: &Path, session_id: &str, issue_id: &str) -> coding::run_registry::RunRecord {
        coding::run_registry::RunRecord {
            session_id: session_id.to_string(),
            account_id: "acct-1".to_string(),
            agent: coding::CodingAgent::Claude,
            kind: coding::run_registry::RunKind::Issue,
            action_id: String::new(),
            action_name: String::new(),
            team_id: "team-1".to_string(),
            issue_id: Some(issue_id.to_string()),
            issue_identifier: Some("EXP-42".to_string()),
            batch_id: None,
            issues: Vec::new(),
            cwd: cwd.to_path_buf(),
            clone: None,
            repo: None,
            repository_id: None,
            board_id: None,
            branch: Some("exp/EXP-42".to_string()),
            base_branch: Some("master".to_string()),
            claude_session_id: Some("claude-1".to_string()),
            codex_originator: None,
            inputs: Vec::new(),
            model: String::new(),
            effort: String::new(),
            ultracode: false,
            fix: None,
            started_reason: None,
            resumed_from_id: None,
            // EXP-746: a pre-746-shaped record (the resume then re-enters
            // the terminal, which is what these tests exercise).
            transport: None,
            acp_session_id: None,
            agent_native_session_id: None,
            acp_child_pid: None,
            host_pid: None,
            recorded_at: coding::run_registry::now_secs(),
            extra: std::collections::BTreeMap::new(),
        }
    }

    /// The frame's `resume` flag GATES the registry lookup: an unchecked box
    /// starts fresh even with a resumable record sitting right there, and a
    /// checked one resolves the newest record for that issue + account (a
    /// miss — another issue, another account — degrades to a fresh session
    /// seeded with the resume prompt).
    #[test]
    fn the_resume_flag_gates_the_registry_lookup() {
        let dir = std::env::temp_dir().join(format!("exp-cli-resume-{}", uuid::Uuid::new_v4()));
        let cwd = dir.join("worktree");
        std::fs::create_dir_all(&cwd).expect("temp worktree");
        coding::run_registry::record(&dir, issue_record(&cwd, "sess-1", "issue-1"));

        assert_eq!(
            issue_resume_record(&dir, "acct-1", "issue-1", false),
            None,
            "resume: false never relaunches a transcript"
        );
        assert_eq!(
            issue_resume_record(&dir, "acct-1", "issue-1", true)
                .map(|record| record.session_id),
            Some("sess-1".to_string())
        );
        assert_eq!(issue_resume_record(&dir, "acct-1", "issue-2", true), None);
        assert_eq!(issue_resume_record(&dir, "acct-2", "issue-1", true), None);

        let _ = std::fs::remove_dir_all(&dir);
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
            let mut record = issue_record(&cwd, session_id, "issue-1");
            record.set_account(account);
            record
        };
        coding::run_registry::record(&dir, with_account("sess-a", Some("work")));
        coding::run_registry::record(&dir, with_account("sess-b", None));
        coding::run_registry::record(&dir, with_account("sess-c", Some("personal")));
        coding::run_registry::record(&dir, with_account("sess-a", Some("shadow")));

        let ids = |ids: &[&str]| ids.iter().map(|id| id.to_string()).collect::<Vec<_>>();
        let pair = |account: &str| (coding::CodingAgent::Claude, account.to_string());
        // EXP-1137: an ambient run is a run on the `system` login — the pair
        // an ambient sign-out must refuse for — never a skipped row.
        assert_eq!(
            live_run_accounts(&dir, &ids(&["sess-c", "sess-b", "sess-a", "sess-missing"])),
            vec![pair("personal"), pair("system"), pair("shadow")]
        );
        assert!(live_run_accounts(&dir, &[]).is_empty());
        assert_eq!(live_run_accounts(&dir, &ids(&["sess-b"])), vec![pair("system")]);

        let _ = std::fs::remove_dir_all(&dir);
    }

    // -----------------------------------------------------------------------
    // REV-9: in-flight remote-start reservations
    // -----------------------------------------------------------------------

    fn issue_subject(id: &str) -> RemoteStartSubject {
        RemoteStartSubject::Issue(id.to_string())
    }

    #[test]
    fn a_duplicate_frame_is_refused_while_the_first_is_in_flight() {
        let reservations = StartReservations::default();
        let first = reservations
            .claim(reservation_keys(&issue_subject("EXP-42")))
            .expect("first frame claims");
        let clash = reservations
            .claim(reservation_keys(&issue_subject("EXP-42")))
            .err()
            .expect("the retry frame must be refused while prepare runs");
        assert_eq!(clash, "issue:EXP-42");
        drop(first);
        assert!(
            reservations
                .claim(reservation_keys(&issue_subject("EXP-42")))
                .is_ok(),
            "the handler returning (success or failure) releases the claim"
        );
    }

    #[test]
    fn unrelated_subjects_start_concurrently() {
        let reservations = StartReservations::default();
        let _a = reservations
            .claim(reservation_keys(&issue_subject("EXP-1")))
            .expect("first issue");
        assert!(reservations.claim(reservation_keys(&issue_subject("EXP-2"))).is_ok());
        assert!(reservations
            .claim(reservation_keys(&RemoteStartSubject::Action {
                action_id: "act-1".to_string(),
                action_name: "Fix merge conflicts".to_string(),
                team_id: "team-1".to_string(),
                repo: None,
                inputs: Vec::new(),
            }))
            .is_ok());
    }

    #[test]
    fn an_overlapping_batch_claim_is_all_or_nothing() {
        let reservations = StartReservations::default();
        let _held = reservations
            .claim(reservation_keys(&issue_subject("EXP-2")))
            .expect("single-issue start in flight");
        let batch = RemoteStartSubject::Batch {
            issue_ids: vec!["EXP-1".to_string(), "EXP-2".to_string()],
            team_id: "team-1".to_string(),
            repo: steer::StartRepoGroup {
                repository_id: "repo-1".to_string(),
                full_name: "niach/exponential".to_string(),
                default_branch: "master".to_string(),
            },
        };
        let clash = reservations
            .claim(reservation_keys(&batch))
            .err()
            .expect("a batch overlapping an in-flight issue start is refused");
        assert_eq!(clash, "issue:EXP-2");
        // The refusal held NOTHING — the batch's other issue stays startable.
        assert!(reservations.claim(reservation_keys(&issue_subject("EXP-1"))).is_ok());
    }

    // -----------------------------------------------------------------------
    // EXP-530: the automation host's pure helpers
    // -----------------------------------------------------------------------

    /// One synced `actions` row carrying `triggers`.
    fn action_row(id: &str, triggers: serde_json::Value) -> domain::rows::ActionRow {
        serde_json::from_value(serde_json::json!({
            "id": id,
            "team_id": "team-1",
            "name": format!("Action {id}"),
            "triggers": triggers,
        }))
        .expect("action row decodes")
    }

    /// One trigger element. `device` is the steer id it binds to.
    fn trigger(id: &str, device: &str, enabled: bool, when: serde_json::Value) -> serde_json::Value {
        let mut element = when;
        element["id"] = serde_json::json!(id);
        element["deviceId"] = serde_json::json!(device);
        element["enabled"] = serde_json::json!(enabled);
        element
    }

    fn daily(minute: u32) -> serde_json::Value {
        serde_json::json!({"kind": "schedule", "interval": "daily", "minuteOfDay": minute})
    }

    /// Only THIS device's ENABLED, readable triggers become engine input.
    #[test]
    fn triggered_actions_keeps_only_this_devices_triggers() {
        let actions = vec![
            action_row(
                "act-1",
                serde_json::json!([
                    trigger("auto-mine", "cli-1", true, daily(420)),
                    // Another machine's binding — that host owns it.
                    trigger("auto-theirs", "desk-9", true, daily(420)),
                ]),
            ),
            action_row(
                "act-2",
                serde_json::json!([
                    // Switched off: inert before the engine ever sees it.
                    trigger("auto-off", "cli-1", false, daily(420)),
                    // A FUTURE kind is unreadable here: skipped.
                    trigger("auto-future", "cli-1", true, serde_json::json!({"kind": "cron"})),
                    trigger(
                        "auto-event",
                        "cli-1",
                        true,
                        serde_json::json!({"kind": "event", "source": "exponential",
                                           "event": "created"}),
                    ),
                ]),
            ),
        ];
        let mine = triggered_actions(&actions, "cli-1");
        assert_eq!(
            mine.iter()
                .map(|entry| entry.triggered.automation_id.as_str())
                .collect::<Vec<_>>(),
            vec!["auto-mine", "auto-event"]
        );
        assert_eq!(mine[0].triggered.action_id, "act-1");
        assert_eq!(mine[0].team_id, "team-1", "the launch needs the action's team");
        assert_eq!(mine[0].name, "Action act-1", "the log prints the action's name");
        assert_eq!(mine[1].triggered.action_id, "act-2");

        // The fingerprint hashes the WHEN-part's VALUE — key order
        // (Electric's jsonb round-trip) and the runner half must not read as
        // an edit, or the GUI and the CLI would re-seed each other's state
        // forever (and an upgrade from the automations row would reseed).
        let reordered = action_row(
            "act-1",
            serde_json::json!([{
                "minuteOfDay": 420, "enabled": true, "interval": "daily",
                "deviceId": "cli-1", "kind": "schedule", "id": "auto-mine", "agent": "codex",
            }]),
        );
        let again = triggered_actions(&[reordered], "cli-1");
        assert_eq!(again[0].triggered.fingerprint, mine[0].triggered.fingerprint);
        assert_eq!(
            mine[0].triggered.fingerprint,
            coding::automations::trigger_fingerprint(&daily(420))
        );
    }

    /// EXP-583: the pins ride from the trigger to the launch options.
    #[test]
    fn triggered_actions_carry_the_launch_pins() {
        let mut pinned = trigger("auto-1", "cli-1", true, daily(420));
        pinned["agent"] = serde_json::json!("codex");
        pinned["model"] = serde_json::json!("gpt-5.1-codex");
        let resolved = triggered_actions(&[action_row("act-1", serde_json::json!([pinned]))], "cli-1");
        assert_eq!(resolved[0].agent.as_deref(), Some("codex"));
        assert_eq!(resolved[0].model.as_deref(), Some("gpt-5.1-codex"));
        // An unpinned effort stays None — the device's default wins.
        assert_eq!(resolved[0].effort, None);
    }

    fn event(kind: &str, payload: serde_json::Value) -> coding::automations::EventRow {
        coding::automations::EventRow {
            id: "evt-1".to_string(),
            issue_id: "issue-1".to_string(),
            team_id: Some("team-1".to_string()),
            created_at_ms: 0,
            kind: kind.to_string(),
            payload: Some(payload),
            board_id: Some("board-1".to_string()),
        }
    }

    fn lookups() -> EventLookups {
        EventLookups {
            issues: HashMap::from([(
                "issue-1".to_string(),
                ("EXP-42".to_string(), "Fix the thing".to_string()),
            )]),
            labels: HashMap::from([("lbl-1".to_string(), "bug".to_string())]),
            statuses: HashMap::from([
                ("st-1".to_string(), "In Progress".to_string()),
                ("st-2".to_string(), "In Review".to_string()),
            ]),
        }
    }

    /// One line per event kind — the prompt grammar the agent reads.
    #[test]
    fn event_lines_render_per_kind() {
        let lookups = lookups();
        let line = |row: coding::automations::EventRow| event_line(&row, &lookups).expect("renders");
        assert_eq!(
            line(event(
                "status_changed",
                serde_json::json!({
                    "from": "in_progress", "to": "in_review",
                    "fromStatusId": "st-1", "toStatusId": "st-2",
                    "fromName": "In Progress", "toName": "In Review"
                })
            )),
            "EXP-42 \"Fix the thing\" status In Progress → In Review"
        );
        // No name snapshot (an older row): the synced status rows fill in.
        assert_eq!(
            line(event(
                "status_changed",
                serde_json::json!({"from": "in_progress", "to": "in_review",
                                   "fromStatusId": "st-1", "toStatusId": "st-2"})
            )),
            "EXP-42 \"Fix the thing\" status In Progress → In Review"
        );
        // Neither: the anchor enum munges (the chain never fails).
        assert_eq!(
            line(event(
                "status_changed",
                serde_json::json!({"from": "in_progress", "to": "in_review"})
            )),
            "EXP-42 \"Fix the thing\" status in progress → in review"
        );
        assert_eq!(
            line(event("priority_changed", serde_json::json!({"from": "low", "to": "urgent"}))),
            "EXP-42 \"Fix the thing\" priority low → urgent"
        );
        assert_eq!(
            line(event("created", serde_json::json!({"priority": "urgent"}))),
            "EXP-42 \"Fix the thing\" created (urgent)"
        );
        // A cleared/absent priority reads `none`.
        assert_eq!(
            line(event("priority_changed", serde_json::json!({"from": "low"}))),
            "EXP-42 \"Fix the thing\" priority low → none"
        );
        assert_eq!(
            line(event("label_added", serde_json::json!({"labelId": "lbl-1"}))),
            "EXP-42 \"Fix the thing\" label bug added"
        );
        // An unsynced label degrades to its id rather than dropping the line.
        assert_eq!(
            line(event("label_added", serde_json::json!({"labelId": "lbl-9"}))),
            "EXP-42 \"Fix the thing\" label lbl-9 added"
        );
        assert_eq!(
            line(event("assignee_changed", serde_json::json!({}))),
            "EXP-42 \"Fix the thing\" assignee changed"
        );
        assert_eq!(
            line(event("pr_opened", serde_json::json!({}))),
            "EXP-42 \"Fix the thing\" pull request opened"
        );
        assert_eq!(
            line(event("pr_merged", serde_json::json!({}))),
            "EXP-42 \"Fix the thing\" pull request merged"
        );

        // An issue this device has not synced names nothing — skip the line.
        let mut orphan = event("created", serde_json::json!({"priority": "urgent"}));
        orphan.issue_id = "issue-unknown".to_string();
        assert_eq!(event_line(&orphan, &lookups), None);
    }

    /// Postgres/Electric timestamp forms → the watermark's ms scale.
    #[test]
    fn event_timestamps_parse_in_both_wire_forms() {
        let expected = 1_754_395_200_000;
        assert_eq!(parse_epoch_ms("2025-08-05T12:00:00.000Z"), Some(expected));
        assert_eq!(parse_epoch_ms("2025-08-05T12:00:00Z"), Some(expected));
        assert_eq!(parse_epoch_ms("2025-08-05 12:00:00+00"), Some(expected));
        assert_eq!(parse_epoch_ms("2025-08-05 12:00:00+00:00"), Some(expected));
        assert_eq!(parse_epoch_ms(""), None);
        assert_eq!(parse_epoch_ms("not-a-date"), None);
    }

    /// EXP-562: the SQL pre-filter's bound is UTC, second-granular, and a
    /// margin BELOW the catch-up cutoff — a byte compare against either wire
    /// form must never exclude a row the exact filter would keep.
    #[test]
    fn event_scan_bound_renders_utc_seconds_minus_margin() {
        // 2025-08-05 12:00:00Z, in a zone whose local time is irrelevant.
        let now_ms = 1_754_395_200_000;
        let bound = event_scan_bound(now_ms);
        assert_eq!(bound, "2025-08-04 11:00:00");
        assert_eq!(
            parse_epoch_ms(&format!("{bound}+00")),
            Some(now_ms - domain::contract::AUTOMATION_EVENT_CATCHUP_MS - EVENT_SCAN_MARGIN_MS),
            "the bound IS the cutoff minus the margin, to the second"
        );
        // Both wire forms of a row exactly AT the cutoff sort above it.
        let cutoff_ms = now_ms - domain::contract::AUTOMATION_EVENT_CATCHUP_MS;
        let at_cutoff = chrono::DateTime::from_timestamp_millis(cutoff_ms).unwrap();
        assert!(at_cutoff.format("%Y-%m-%d %H:%M:%S%.3f+00").to_string() > bound);
        assert!(at_cutoff.to_rfc3339() > bound);
    }

    /// Only an `issue_events` batch invalidates the cached snapshot — ticks
    /// and trigger edits re-decide over the rows already in hand.
    #[test]
    fn only_an_events_batch_forces_a_rescan() {
        assert!(!rescan_events(&[AutomationWork::Tick]));
        assert!(!rescan_events(&[
            AutomationWork::Tick,
            AutomationWork::ActionsChanged
        ]));
        assert!(rescan_events(&[
            AutomationWork::Tick,
            AutomationWork::EventsChanged,
            AutomationWork::ActionsChanged,
        ]));
        // An empty drain can't happen (recv yields the first item), but the
        // fold must still be total.
        assert!(!rescan_events(&[]));
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
        for cap in coding::ACTION_CAPS {
            assert!(caps.contains(&cap.to_string()), "missing action cap {cap}");
        }
        // Nothing runnable = build caps only.
        let signed_out = device_caps(&advert(&[]));
        assert_eq!(signed_out.len(), coding::DEVICE_CAPS.len());
        assert!(signed_out.contains(&"agent-login".to_string()));
        assert!(!signed_out.contains(&"automations".to_string()));
    }

    /// The trigger host's shapes are real shapes, and deliberately NOT the
    /// desktop's whole set.
    #[test]
    fn automation_shapes_are_a_real_subset() {
        assert!(AUTOMATION_SHAPES.len() < 20);
        for shape in AUTOMATION_SHAPES {
            assert!(
                sync::shapes::shape_by_name(shape).is_some(),
                "{shape} is not a real shape"
            );
        }
    }

    #[test]
    fn a_panicking_handler_still_releases_its_claim() {
        let reservations = StartReservations::default();
        let inner = reservations.clone();
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
            let _guard = inner
                .claim(reservation_keys(&issue_subject("EXP-9")))
                .expect("claims before the panic");
            panic!("prepare blew up");
        }));
        assert!(
            reservations.claim(reservation_keys(&issue_subject("EXP-9"))).is_ok(),
            "a crashed start must not wedge its issue until daemon restart"
        );
    }
}
