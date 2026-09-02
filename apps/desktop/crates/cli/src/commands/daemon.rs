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
use crate::sidecars::Sidecars;

/// EXP-481: 30s (down from 60) — online-ness now derives from
/// `last_seen_at` freshness against the contract's 90s window, so one
/// missed beat must not flap the badge.
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(30);
/// EXP-641: how often a 426-gated daemon re-tries the self-update while the
/// gate holds and no newer release was installable yet (the release assets
/// can still be uploading when the web deploy that raised the floor lands).
const GATED_UPDATE_RETRY: Duration = Duration::from_secs(5 * 60);
const DOCTOR_RECHECK: Duration = Duration::from_secs(5 * 60);
/// EXP-414: a changed agent advertisement is only ACTED on once a second
/// probe agrees ([`advert_transition`]) — this is the shortened recheck that
/// confirms (or clears) a pending change, so a real change still converges
/// in ~DOCTOR_RECHECK + this instead of two full periods.
const ADVERT_CONFIRM_RECHECK: Duration = Duration::from_secs(30);
/// EXP-481 adds `resume` (start_session honors the resume flag — registry
/// driven since EXP-662: a recorded run relaunches its exact transcript, a
/// record-less issue degrades to a fresh resume-prompted session),
/// `worktrees` (inventory reporting + remove/prune commands) and
/// `launch-defaults` (server-authoritative defaults convergence + the
/// `check_in` nudge frame). EXP-490 split them off [`ACTION_CAPS`]: these
/// three are BUILD capabilities and ride even with zero runnable agents.
/// EXP-484 adds `agent-login`: this build executes the `agent_login` device
/// command (run the agent's own sign-in in a PTY, report the link back).
/// It is deliberately a BUILD cap — signing IN is exactly what a machine
/// with no runnable agent needs. EXP-679 adds `agent-start`: this build
/// understands the `started_reason` field on a `StartSession` frame and
/// forwards it into `codingSessions.start`, so a run another coding session
/// asked for (MCP `exponential_sessions_start`, which sends the parent's id)
/// lands UNATTENDED here instead of silently degrading to a person-started
/// run that nothing ever closes out. It is a PROTOCOL cap, so build-level:
/// the server refuses an agent-parented start against a device without it.
/// Hand-synced with the desktop's `steer_wiring::device_caps` vec.
pub const DEVICE_CAPS: [&str; 5] = [
    "resume",
    "worktrees",
    "launch-defaults",
    "agent-login",
    "agent-start",
];

/// The action-run capabilities — advertised only while at least one agent is
/// RUNNABLE (EXP-409: a machine whose only agents are signed out cannot run
/// actions either). EXP-530 adds `automations`: this daemon evaluates the
/// triggers bound to its device id and starts the runs itself, so the web
/// device pickers may offer it as an automation host (hand-synced with the
/// desktop's `steer_wiring` caps vec).
/// EXP-615 adds `chat`: this build runs the hidden `builtin:chat` action, so
/// the remote Chat tab may target this machine.
/// EXP-637 adds `resume-run`: this build can resume an ended action/chat run
/// out of its own run registry.
pub const ACTION_CAPS: [&str; 6] = [
    "actions",
    "action-inputs",
    "fix-conflicts",
    "automations",
    "chat",
    "resume-run",
];

/// The caps to advertise for a doctor snapshot: the build caps, plus the
/// action caps while anything is runnable.
fn device_caps(advertised: &coding::AgentAdvertisement) -> Vec<String> {
    let mut caps: Vec<String> = DEVICE_CAPS.iter().map(|cap| cap.to_string()).collect();
    if !advertised.agents.is_empty() {
        caps.extend(ACTION_CAPS.iter().map(|cap| cap.to_string()));
    }
    caps
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
    /// EXP-530: the `actions` row this run executes (from the prepared
    /// launch's [`terminal::tab::TabKind::Action`]) — the automation host's
    /// defer check ("never launch a second run of an action already
    /// running here").
    action_id: Option<String>,
    branch: String,
    is_fix_run: bool,
    /// EXP-637: the run's own worktree, reclaimed by the reaper when the
    /// session finishes clean. `None` for issue/batch sessions (their
    /// worktrees survive by design) and repo-less runs.
    cleanup: Option<coding::RunCleanup>,
    session: Arc<RunningSession>,
}

type Sessions = Arc<Mutex<Vec<LiveSession>>>;

fn lock_sessions(sessions: &Sessions) -> std::sync::MutexGuard<'_, Vec<LiveSession>> {
    match sessions.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

/// Local one-session-per-issue dedup — only STILL-RUNNING sessions count
/// (a finished entry awaiting the reaper tick must not block a restart).
fn issue_is_coding_here(sessions: &Sessions, issue_id: &str) -> bool {
    lock_sessions(sessions).iter().any(|live| {
        live.issue_id.as_deref() == Some(issue_id) && !live.session.is_done()
    })
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

    // EXP-229 parity: end orphaned rows a previous crash left `running`
    // (pid-guarded — rows owned by a live sibling process are skipped).
    reconcile_stale_sessions(&ctx);

    let sidecars = Arc::new(Sidecars::start());
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
    let device_worker = spawn_device_worker(
        Arc::clone(&ctx),
        Arc::clone(&sessions),
        device_id.clone(),
        Arc::clone(&agent_status),
        Arc::clone(&doctor_soon),
    );
    let check_in = Arc::new(AtomicBool::new(false));
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
            sidecars: Arc::clone(&sidecars),
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
    // What the two jsonb columns last SENT said: a beat attaches a map only
    // when it actually changed, so the steady-state body stays tiny and
    // `agent_usage_at` (which the server stamps on every write) does not move
    // on every beat. Accounts compare on `accounts_key` — their IDENTITY —
    // because every collection pass restamps `checkedAt`, so a JSON compare
    // would call an unchanged map "changed" on every single beat.
    let mut sent_accounts: Option<String> = None;
    let mut sent_usage: Option<String> = None;
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
        log::info!("no agent CLI installed — registered offline; install claude/codex/pi to accept remote starts");
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
    while !shutdown_requested() {
        match inbox_rx.recv_timeout(Duration::from_secs(1)) {
            // REV-9: claim the frame's subject BEFORE spawning its thread —
            // the claim is the only dedup a duplicate frame can hit while the
            // first is still preparing (see [`StartReservations`]).
            Ok(start) => match reservations.claim(reservation_keys(&start.subject)) {
                Err(clash) => {
                    log::info!("remote start ignored — a start holding {clash} is already in flight");
                }
                Ok(reservation) => {
                    let ctx = Arc::clone(&ctx);
                    let sidecars = Arc::clone(&sidecars);
                    let runtime = runtime.clone();
                    let sessions = Arc::clone(&sessions);
                    let personal_key = personal_key.clone();
                    let device_id = device_id.clone();
                    std::thread::spawn(move || {
                        let _reservation = reservation;
                        handle_remote_start(
                            &ctx,
                            &sidecars,
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
        {
            let mut guard = lock_sessions(&sessions);
            let reaped: Vec<(String, coding::RunCleanup)> = guard
                .iter()
                .filter(|live| live.session.is_done())
                .filter_map(|live| {
                    live.cleanup
                        .clone()
                        .map(|cleanup| (live.session.session_id.clone(), cleanup))
                })
                .collect();
            guard.retain(|live| !live.session.is_done());
            drop(guard);
            for (session_id, cleanup) in reaped {
                let verdict = coding::remove_if_clean(&cleanup);
                if matches!(verdict, coding::CleanupOutcome::Removed) {
                    coding::run_registry::remove(&ctx.data_dir, &session_id);
                }
                log::info!("run cleanup [{session_id}] on {}: {verdict:?}", cleanup.branch);
            }
        }

        let live_now = lock_sessions(&sessions).len();
        let session_change = reported_sessions != Some(live_now);
        // EXP-481: a relay check_in nudge means "the server persisted new
        // work" — beat NOW instead of on the cadence (the beat is the pull).
        let nudged = check_in.swap(false, Ordering::SeqCst);
        if last_heartbeat.elapsed() >= HEARTBEAT_INTERVAL || session_change || nudged {
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
            let send_accounts = accounts_key.is_some() && accounts_key != sent_accounts;
            let send_usage = usage_text.is_some() && usage_text != sent_usage;
            match api::devices::heartbeat(
                &ctx.trpc,
                &api::devices::HeartbeatInput {
                    device_id: &device_id,
                    active_sessions: live_now as u32,
                    defaults_synced_at: synced_at.as_deref(),
                    agent_accounts: send_accounts.then_some(accounts_json.as_ref()).flatten(),
                    agent_usage: send_usage.then_some(usage_json.as_ref()).flatten(),
                },
            ) {
                Ok(result) => {
                    // Only an ACCEPTED beat updates the last-sent copies: a
                    // failed one must resend on the next tick.
                    if send_accounts {
                        sent_accounts = accounts_key.clone();
                    }
                    if send_usage {
                        sent_usage = usage_text.clone();
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
                                "update requested from the web — parked until {live_now} live session(s) close"
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
                Err(err) => log::debug!("devices.heartbeat failed: {err}"),
            }
            // EXP-484: collect for the NEXT beat, on the worker — never
            // here (a codex app-server probe blocks for seconds, and this
            // loop also owns remote starts).
            device_worker
                .send(DeviceWork::CollectAgentStatus {
                    report: Box::new(doctor.clone()),
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
                    "server rejected this build (426) — updating once {live} live session(s) close (a gated run ends itself once the server has swept its row, EXP-681)"
                );
            } else {
                log::info!("server rejected this build (426) — updating now");
            }
            pending_update = Some(UpdateTrigger::Gated);
        }
        if let Some(trigger) = pending_update {
            let idle = lock_sessions(&sessions).is_empty();
            if idle {
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
    let _ = std::fs::remove_file(pidfile(&ctx.data_dir));
    Ok(ExitCode::SUCCESS)
}

/// One doctor pass → what the relay/registry hears, plus the report itself.
/// EXP-484: the accounts map and the usage collector both read the REPORT
/// (which binaries exist, who is signed in), so the daemon keeps the last
/// one beside the advertisement instead of re-probing for it.
fn probe_agents(ctx: &Ctx) -> (coding::AgentAdvertisement, coding::DoctorReport) {
    let settings = coding::Settings::load(&coding::Settings::default_path(&ctx.data_dir));
    let report = coding::run_doctor(&settings);
    let advertisement = report.agent_advertisement(&settings);
    (advertisement, report)
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
    let accounts = doctor.agent_accounts(&coding::now_iso());
    let agent_accounts = (!accounts.is_empty())
        .then(|| serde_json::to_value(&accounts).ok())
        .flatten();
    let result = api::devices::register(
        &ctx.trpc,
        &api::devices::RegisterDevice {
            device_id,
            label: device_label,
            kind: "server",
            platform: Some(std::env::consts::OS),
            agents: &advertised.agents,
            unauthed_agents: &advertised.unauthed_agents,
            caps: &caps,
            launch_defaults: Some(&launch_defaults),
            // EXP-484 (A3): who is signed in where, straight off the last
            // doctor pass — the usage windows are a heartbeat concern (they
            // need the collector), the accounts map is not, and a
            // just-registered machine should already say "signed in as …".
            agent_accounts: agent_accounts.as_ref(),
            version: Some(crate::cli_version()),
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
    let control_api: Arc<dyn ControlApi> = Arc::new(TrpcControlApi(Arc::clone(&ctx.trpc)));
    Some(steer::spawn_control_channel(
        runtime,
        device,
        control_api,
        on_start,
        on_check_in,
    ))
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
// Remote-start dispatch (steer_wiring's handle_remote_start, headless)
// ---------------------------------------------------------------------------

fn handle_remote_start(
    ctx: &Ctx,
    sidecars: &Sidecars,
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
    );
    let origin = coding::LaunchOrigin::Relay {
        device_id: device_id.to_string(),
        claimant: ctx.account.id.clone(),
        started_by: start.started_by.clone(),
        // EXP-679: `agent` — another coding session asked for this start, so
        // the run is unattended and its close-out ends it.
        started_reason: start.started_reason.clone(),
    };

    // Errors and refusals are logged, never acked — the remote client
    // observes success purely via the synced `coding_sessions` row
    // appearing (desktop parity).
    let outcome = match start.subject.clone() {
        RemoteStartSubject::Issue(issue_id) => remote_issue_start(
            ctx, sidecars, runtime, sessions, personal_key, options, origin, issue_id,
            // EXP-481: honor the remote resume flag — the launcher's marker
            // gate degrades a missing/foreign worktree to a fresh session.
            start.resume,
        ),
        RemoteStartSubject::Batch { issue_ids, team_id, repo } => remote_batch_start(
            ctx, sidecars, runtime, sessions, personal_key, options, origin, issue_ids, team_id, repo,
        ),
        RemoteStartSubject::Action { action_id, team_id, repo, inputs, .. } => remote_action_start(
            ctx, sidecars, runtime, sessions, personal_key, options, origin, action_id, team_id, repo, inputs,
        ),
        // EXP-637: the run registry holds everything else (agent, workspace,
        // branch, options), so the frame's launch options are ignored by
        // contract — a resumed run keeps what it recorded.
        RemoteStartSubject::Resume { session_id } => remote_resume_start(
            ctx, sidecars, runtime, sessions, personal_key, origin, session_id,
        ),
    };
    if let Err(err) = outcome {
        log::warn!("remote start failed: {err:#}");
    }
}

/// The one-session-per-issue guards every issue-shaped start takes — this
/// daemon's own live sessions first, then the REV2-24 cross-device probe
/// (desktop parity: one session per issue, wherever it runs). `Some` refuses
/// the start and carries the reason; the probe is best-effort, so an older
/// server without it never blocks.
fn issue_start_blocker(ctx: &Ctx, sessions: &Sessions, issue_id: &str) -> Option<String> {
    if issue_is_coding_here(sessions, issue_id) {
        return Some(format!(
            "remote start for {issue_id} ignored — already coding this issue"
        ));
    }
    if let Ok(Some(live)) = api::coding_sessions::live_for_issue(&ctx.trpc, issue_id) {
        return Some(format!(
            "remote start for {issue_id} ignored — live session on {} (one session per issue)",
            live.device_label.as_deref().unwrap_or("another device")
        ));
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
    if !start_resume {
        return None;
    }
    coding::run_registry::latest_for_issue(data_dir, account_id, issue_id)
}

#[allow(clippy::too_many_arguments)]
fn remote_issue_start(
    ctx: &Ctx,
    sidecars: &Sidecars,
    runtime: Option<&Arc<steer::SteerRuntime>>,
    sessions: &Sessions,
    personal_key: Option<String>,
    options: LaunchOptions,
    origin: coding::LaunchOrigin,
    issue_id: String,
    start_resume: bool,
) -> anyhow::Result<()> {
    if let Some(reason) = issue_start_blocker(ctx, sessions, &issue_id) {
        log::info!("{reason}");
        return Ok(());
    }
    let fetched = api::issues::issues_get(&ctx.trpc, &issue_id).context("resolve the issue")?;
    let issue = fetched.issue;
    let mut seeds = HashMap::new();
    seeds.insert(issue.id.clone(), launch::issue_seed(&issue));
    let deps = launch::coding_deps(ctx, seeds, launch::LaunchHost::Daemon);
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
        }),
        None => PrepareRequest::Issue(launch::issue_launch_request(
            &issue,
            options,
            origin,
            start_resume,
        )),
    };
    let prepared = coding::prepare_with_hooks(
        &request,
        &deps,
        sidecars.hook_setup().as_ref(),
        sidecars.observer_setup().as_ref(),
    )
    .map_err(|err| anyhow::anyhow!("{err}"))?;
    spawn_prepared(
        ctx, sidecars, runtime, sessions, personal_key, prepared,
        Some(issue.id), false,
    )
}

#[allow(clippy::too_many_arguments)]
fn remote_batch_start(
    ctx: &Ctx,
    sidecars: &Sidecars,
    runtime: Option<&Arc<steer::SteerRuntime>>,
    sessions: &Sessions,
    personal_key: Option<String>,
    options: LaunchOptions,
    origin: coding::LaunchOrigin,
    issue_ids: Vec<String>,
    team_id: String,
    repo: steer::StartRepoGroup,
) -> anyhow::Result<()> {
    let mut issues = Vec::new();
    let mut seeds = HashMap::new();
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
        repo: coding::RepoGroup {
            repository_id: repo.repository_id,
            full_name: repo.full_name,
            default_branch: repo.default_branch,
        },
        issues,
        device_label: coding::default_device_label(),
        origin,
        options,
    };
    let deps = launch::coding_deps(ctx, seeds, launch::LaunchHost::Daemon);
    let prepared = coding::prepare_with_hooks(
        &PrepareRequest::Batch(request),
        &deps,
        sidecars.hook_setup().as_ref(),
        sidecars.observer_setup().as_ref(),
    )
    .map_err(|err| anyhow::anyhow!("{err}"))?;
    spawn_prepared(ctx, sidecars, runtime, sessions, personal_key, prepared, None, false)
}

#[allow(clippy::too_many_arguments)]
fn remote_action_start(
    ctx: &Ctx,
    sidecars: &Sidecars,
    runtime: Option<&Arc<steer::SteerRuntime>>,
    sessions: &Sessions,
    personal_key: Option<String>,
    options: LaunchOptions,
    origin: coding::LaunchOrigin,
    action_id: String,
    team_id: String,
    repo: Option<steer::StartRepoGroup>,
    inputs: Vec<steer::StartInput>,
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

    let deps = launch::coding_deps(ctx, HashMap::new(), launch::LaunchHost::Daemon);
    let prepared = coding::prepare_with_hooks(
        &PrepareRequest::Action(request),
        &deps,
        sidecars.hook_setup().as_ref(),
        sidecars.observer_setup().as_ref(),
    )
    .map_err(|err| anyhow::anyhow!("{err}"))?;
    spawn_prepared(ctx, sidecars, runtime, sessions, personal_key, prepared, None, is_fix_run)
}

/// EXP-637 — RESUME an ended run of ANY kind out of the local run registry
/// (EXP-662 added the issue and batch shapes). A record this daemon never
/// wrote (or whose workspace is gone) is a hard refusal, logged: the requester
/// sees no new session row, exactly like every other refused start.
#[allow(clippy::too_many_arguments)]
fn remote_resume_start(
    ctx: &Ctx,
    sidecars: &Sidecars,
    runtime: Option<&Arc<steer::SteerRuntime>>,
    sessions: &Sessions,
    personal_key: Option<String>,
    origin: coding::LaunchOrigin,
    session_id: String,
) -> anyhow::Result<()> {
    let Some(record) = coding::run_registry::get(&ctx.data_dir, &session_id) else {
        anyhow::bail!("no local record for run {session_id} — it ran on another machine");
    };
    if !record.resumable() {
        anyhow::bail!("run {session_id}'s workspace is gone");
    }
    // EXP-662: an issue/batch record resumes as a SESSION on those issues, so
    // it takes the same one-session-per-issue guards a fresh start does.
    let mut seeds = HashMap::new();
    match record.kind {
        coding::run_registry::RunKind::Issue => {
            if let Some(issue_id) = record.issue_id.clone() {
                if let Some(reason) = issue_start_blocker(ctx, sessions, &issue_id) {
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
                if let Some(reason) = issue_start_blocker(ctx, sessions, &issue.issue_id) {
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
    };
    let deps = launch::coding_deps(ctx, seeds, launch::LaunchHost::Daemon);
    let prepared = coding::prepare_with_hooks(
        &PrepareRequest::ResumeRun(request),
        &deps,
        sidecars.hook_setup().as_ref(),
        sidecars.observer_setup().as_ref(),
    )
    .map_err(|err| anyhow::anyhow!("{err}"))?;
    spawn_prepared(ctx, sidecars, runtime, sessions, personal_key, prepared, issue_id, false)
}

#[allow(clippy::too_many_arguments)]
fn spawn_prepared(
    ctx: &Ctx,
    sidecars: &Sidecars,
    runtime: Option<&Arc<steer::SteerRuntime>>,
    sessions: &Sessions,
    personal_key: Option<String>,
    prepared: Prepared,
    issue_id: Option<String>,
    is_fix_run: bool,
) -> anyhow::Result<()> {
    let prepared = match prepared {
        Prepared::Ready(prepared) => prepared,
        Prepared::Disabled(reason) => {
            log::warn!("remote start refused: {}", reason.message());
            return Ok(());
        }
    };
    let env = LaunchEnv {
        ctx,
        runtime,
        sidecars,
        personal_key,
    };
    // EXP-530: an action run's own id, straight off the prepared tab kind —
    // the automation host defers while it is live.
    let action_id = match &prepared.tab_kind {
        terminal::tab::TabKind::Action(id) => Some(id.clone()),
        _ => None,
    };
    // EXP-637: snapshotted before the launch consumes the prepared value.
    let cleanup = prepared.run_cleanup.clone();
    let session = session_host::launch(&env, prepared, false, issue_id.clone())?;
    log::info!(
        "session {} started ({}, branch {})",
        session.session_id,
        session.issue_identifier,
        session.branch
    );
    lock_sessions(sessions).push(LiveSession {
        issue_id,
        action_id,
        branch: session.branch.clone(),
        is_fix_run,
        cleanup,
        session: Arc::new(session),
    });
    Ok(())
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
    /// (boxed — it dwarfs every other variant).
    CollectAgentStatus { report: Box<coding::DoctorReport> },
}

fn spawn_device_worker(
    ctx: Arc<Ctx>,
    sessions: Sessions,
    device_id: String,
    agent_status: Arc<Mutex<Option<coding::AgentStatusPayload>>>,
    doctor_soon: Arc<AtomicBool>,
) -> flume::Sender<DeviceWork> {
    let (tx, rx) = flume::unbounded::<DeviceWork>();
    std::thread::spawn(move || {
        let mut last_inventory_fp: Option<u64> = None;
        // EXP-484: `agent_login` runs on its own thread (a PTY that lives
        // for minutes must not block this worker) — the set is what makes a
        // REDELIVERED command id a no-op instead of a second sign-in.
        let logins_inflight: Arc<Mutex<HashSet<String>>> = Arc::new(Mutex::new(HashSet::new()));
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
                            &doctor_soon,
                        );
                    }
                    report_worktrees(&ctx, &sessions, &device_id, &mut last_inventory_fp);
                }
                DeviceWork::ReportWorktrees => {
                    report_worktrees(&ctx, &sessions, &device_id, &mut last_inventory_fp);
                }
                DeviceWork::CollectAgentStatus { report } => {
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
                }
            }
        }
    });
    tx
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
    let changed = coding::apply_defaults_patch(&mut settings, &patch);
    if changed {
        if let Err(err) = settings.save(&settings_path) {
            log::warn!("launch defaults: save failed: {err}");
            return;
        }
        log::info!("launch defaults: applied the server copy");
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
    ctx: &Ctx,
    sessions: &Sessions,
    command: &api::devices::PendingCommand,
    logins_inflight: &Arc<Mutex<HashSet<String>>>,
    doctor_soon: &Arc<AtomicBool>,
) {
    let settings = coding::Settings::load(&coding::Settings::default_path(&ctx.data_dir));
    let repos_root = settings.repos_root_path();
    let held: std::collections::HashSet<String> = lock_sessions(sessions)
        .iter()
        .filter(|live| !live.session.is_done())
        .map(|live| live.branch.clone())
        .collect();
    let (ok, message) = match command.kind.as_str() {
        "worktree_remove" => {
            let repo = command.payload["repoFullName"].as_str().unwrap_or_default();
            let branch = command.payload["branch"].as_str().unwrap_or_default();
            if repo.is_empty() || branch.is_empty() {
                (false, "Malformed command payload.".to_string())
            } else {
                let clone = coding::clone_path(&repos_root, repo);
                match coding::remove_worktree_remote(&clone, branch, &held) {
                    Ok(()) => (true, format!("Removed the {branch} worktree.")),
                    Err(err) => (false, err.message()),
                }
            }
        }
        "worktree_prune" => run_prune(&settings, &repos_root, &ctx.data_dir, held),
        // EXP-484: a sign-in on this machine, requested from anywhere. The
        // PTY lives for minutes, so the host owns its own thread and its own
        // completion — this arm never falls through to the one below.
        "agent_login" => {
            crate::agent_login_host::run(
                ctx,
                settings,
                command.clone(),
                Arc::clone(logins_inflight),
                Arc::clone(doctor_soon),
            );
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

/// The prune command body: the conservative (git-truth-only) policy over
/// every clone; aggregate one human-readable summary.
fn run_prune(
    settings: &coding::Settings,
    repos_root: &std::path::Path,
    data_dir: &std::path::Path,
    held: std::collections::HashSet<String>,
) -> (bool, String) {
    let policy =
        coding::conservative_prune_policy(
            &settings.branch_prefix,
            held,
            Vec::new(),
            // EXP-637: nominate this install's recorded run branches too
            // (git still confirms they landed before anything is removed).
            Some(data_dir.to_path_buf()),
        );
    let mut removed = 0usize;
    let mut skipped = 0usize;
    let mut blocked = false;
    for clone in coding::scan_clones(repos_root) {
        let report = coding::prune_landed(&clone.path, &policy);
        removed += report.removed_worktrees.len();
        skipped += report.skipped.len();
        blocked |= report.blocked_by_launch;
    }
    let mut message = format!("Pruned {removed} worktree{}", if removed == 1 { "" } else { "s" });
    if skipped > 0 {
        message.push_str(&format!(", kept {skipped} (unmerged or busy)"));
    }
    if blocked {
        message.push_str("; one repo was busy launching — try again");
    }
    message.push('.');
    (true, message)
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
// EXP-530: the automation host — this daemon's own Electric pipeline (a
// 6-shape subset in `sync-cli.sqlite`), ONE delta-drain thread, and the
// serialized automations worker. The worker evaluates the triggers bound to
// THIS device (`Ctx::device_id`) through the shared `coding::automations`
// engine and self-starts the fired action runs; the sessions it spawns join
// the normal `Sessions` vec, so heartbeat/parked-update/quit-sweep cover them
// like any other run.
// ---------------------------------------------------------------------------

/// What the automation host syncs: the bindings (`automations`) and the
/// actions they fire (`actions` — the name snapshot the log prints), the
/// event feed (`issue_events`), and the rows the prompt lines and board
/// filters read (`issues`, `boards`, `labels`, `issue_statuses`).
/// Deliberately NOT the desktop's 19 — a headless daemon has no views to
/// hydrate.
const AUTOMATION_SHAPES: &[&str] = &[
    "automations",
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
    /// A synced `automations` (or `actions`) batch landed — an automation
    /// was authored, retargeted or toggled. Re-decide, but the events
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
                    "automations" | "actions" => AutomationWork::ActionsChanged,
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
    sidecars: Arc<Sidecars>,
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

/// One automation bound to this device, plus what the LAUNCH needs beyond
/// the engine's view of it.
#[derive(Clone, Debug)]
struct AutomationAction {
    triggered: coding::automations::TriggeredAutomation,
    team_id: String,
    /// The target action's display name — the log line's handle.
    name: String,
    /// The automation's pinned agent/model/effort; every `None` falls back
    /// to this machine's launch defaults.
    agent: Option<String>,
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
        let automation_rows =
            read_shape_rows::<domain::rows::AutomationRow>(&store, "automations");
        let action_rows = read_shape_rows::<domain::rows::ActionRow>(&store, "actions");
        let actions = triggered_actions(&automation_rows, &action_rows, &self.device_id);
        if actions.is_empty() {
            return;
        }
        let settings_path = coding::Settings::default_path(&self.ctx.data_dir);
        let mut states = coding::automations::read_states(&settings_path, &self.device_id);
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
        match coding::automations::write_states(settings_path, &self.device_id, states) {
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
        )?;
        let deps = launch::coding_deps(&self.ctx, HashMap::new(), launch::LaunchHost::Daemon);
        let prepared = coding::prepare_with_hooks(
            &PrepareRequest::Action(request),
            &deps,
            self.sidecars.hook_setup().as_ref(),
            self.sidecars.observer_setup().as_ref(),
        )
        .map_err(|err| anyhow::anyhow!("{err}"))?;
        if let Prepared::Disabled(reason) = &prepared {
            log::warn!("automation run refused: {}", reason.message());
            return Ok(false);
        }
        spawn_prepared(
            &self.ctx,
            &self.sidecars,
            self.runtime.as_ref(),
            &self.sessions,
            self.personal_key.clone(),
            prepared,
            None,
            false,
        )?;
        Ok(true)
    }
}

/// The synced `automations` rows this device evaluates: ENABLED, bound to
/// `device_id`, with a team and a target action to run. Fingerprints hash the
/// DECODED trigger value — the same canonical input the GUI feeds
/// `trigger_fingerprint` — so the two hosts agree on what counts as an edit.
/// A MALFORMED but targeted trigger survives as `Unsupported` (inert, and the
/// engine ignores it) so the row stays visible instead of vanishing.
fn triggered_actions(
    rows: &[domain::rows::AutomationRow],
    actions: &[domain::rows::ActionRow],
    device_id: &str,
) -> Vec<AutomationAction> {
    rows.iter()
        .filter(|row| row.is_enabled())
        .filter(|row| row.device_id.as_deref() == Some(device_id))
        .filter_map(|row| {
            let trigger = row.trigger.as_ref()?;
            let parsed = coding::automations::parse_trigger(trigger)?;
            let action_id = row.action_id.clone()?;
            // A team-less row has nowhere to run (GUI parity) — and the
            // team doubles as the engine's event fence.
            let team_id = row.team_id.clone()?;
            // The action's own row is only the display name here; the launch
            // re-fetches it fresh (synced rows carry no body).
            let name = actions
                .iter()
                .find(|action| action.id == action_id)
                .and_then(|action| action.name.clone())
                .unwrap_or_default();
            Some(AutomationAction {
                triggered: coding::automations::TriggeredAutomation {
                    automation_id: row.id.clone(),
                    action_id,
                    team_id: team_id.clone(),
                    fingerprint: coding::automations::trigger_fingerprint(trigger),
                    trigger: parsed,
                },
                team_id,
                name,
                agent: row.agent.clone(),
                model: row.model.clone(),
                effort: row.effort.clone(),
            })
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
</dict>
</plist>
"#,
                exe = exe.display()
            ),
        )?;
        let loaded = std::process::Command::new("launchctl")
            .args(["load", "-w"])
            .arg(&plist)
            .status()
            .map(|status| status.success())
            .unwrap_or(false);
        println!("Wrote {}", plist.display());
        if loaded {
            println!("Daemon loaded — it starts at login from now on.");
        } else {
            println!("Load it with: launchctl load -w {}", plist.display());
        }
    } else {
        let unit_dir = dirs::config_dir()
            .context("resolve XDG config dir")?
            .join("systemd/user");
        std::fs::create_dir_all(&unit_dir)?;
        let unit = unit_dir.join("exponential-daemon.service");
        std::fs::write(
            &unit,
            format!(
                "[Unit]\nDescription=Exponential remote-start daemon\nAfter=network-online.target\n\n[Service]\nExecStart={exe} daemon --foreground{label_args_unit}\nRestart=on-failure\nRestartSec=5\n\n[Install]\nWantedBy=default.target\n",
                exe = exe.display()
            ),
        )?;
        println!("Wrote {}", unit.display());
        let enabled = std::process::Command::new("systemctl")
            .args(["--user", "daemon-reload"])
            .status()
            .map(|status| status.success())
            .unwrap_or(false)
            && std::process::Command::new("systemctl")
                .args(["--user", "enable", "--now", "exponential-daemon"])
                .status()
                .map(|status| status.success())
                .unwrap_or(false);
        if enabled {
            println!("Daemon enabled and started (systemd user unit `exponential-daemon`).");
            println!("Survive logout with: loginctl enable-linger $USER");
        } else {
            println!("Enable it with: systemctl --user daemon-reload && systemctl --user enable --now exponential-daemon");
        }
    }
    Ok(ExitCode::SUCCESS)
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
            Ok(ExitCode::FAILURE)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn advert(agents: &[&str]) -> coding::AgentAdvertisement {
        coding::AgentAdvertisement {
            agents: agents.iter().map(|agent| agent.to_string()).collect(),
            unauthed_agents: Vec::new(),
            default_agent: "claude".to_string(),
            launch_defaults: agents
                .iter()
                .map(|agent| (agent.to_string(), coding::AgentLaunchDefaults::default()))
                .collect(),
        }
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
            branch: Some("exp/EXP-42".to_string()),
            base_branch: Some("master".to_string()),
            claude_session_id: Some("claude-1".to_string()),
            pi_session_file: None,
            codex_originator: None,
            inputs: Vec::new(),
            model: String::new(),
            effort: String::new(),
            ultracode: false,
            fix: None,
            started_reason: None,
            resumed_from_id: None,
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

    fn action_row(id: &str) -> domain::rows::ActionRow {
        serde_json::from_value(serde_json::json!({
            "id": id,
            "team_id": "team-1",
            "name": format!("Action {id}"),
        }))
        .expect("action row decodes")
    }

    /// One synced `automations` row. `device` is the steer id it binds to.
    fn automation_row(
        id: &str,
        action_id: &str,
        device: &str,
        enabled: bool,
        trigger: serde_json::Value,
    ) -> domain::rows::AutomationRow {
        serde_json::from_value(serde_json::json!({
            "id": id,
            "team_id": "team-1",
            "action_id": action_id,
            "device_id": device,
            "enabled": enabled,
            "trigger": trigger,
        }))
        .expect("automation row decodes")
    }

    fn daily(minute: u32) -> serde_json::Value {
        serde_json::json!({"kind": "schedule", "interval": "daily", "minuteOfDay": minute})
    }

    /// Only THIS device's ENABLED, evaluable automations become engine input.
    #[test]
    fn triggered_actions_keeps_only_this_devices_rows() {
        let actions = vec![action_row("act-1"), action_row("act-2")];
        let rows = vec![
            automation_row("auto-mine", "act-1", "cli-1", true, daily(420)),
            // Another machine's binding — that host owns it.
            automation_row("auto-theirs", "act-1", "desk-9", true, daily(420)),
            // Switched off: inert before the engine ever sees it.
            automation_row("auto-off", "act-2", "cli-1", false, daily(420)),
            // Malformed but TARGETED: kept, inert (the engine skips
            // Unsupported) so the row stays visible instead of vanishing.
            automation_row(
                "auto-future",
                "act-2",
                "cli-1",
                true,
                serde_json::json!({"kind": "cron"}),
            ),
        ];
        let mine = triggered_actions(&rows, &actions, "cli-1");
        assert_eq!(
            mine.iter()
                .map(|entry| entry.triggered.automation_id.as_str())
                .collect::<Vec<_>>(),
            vec!["auto-mine", "auto-future"]
        );
        assert_eq!(mine[0].triggered.action_id, "act-1");
        assert_eq!(mine[0].team_id, "team-1", "the launch needs the row's team");
        assert_eq!(mine[0].name, "Action act-1", "the log prints the action's name");
        assert_eq!(
            mine[1].triggered.trigger.kind,
            coding::automations::TriggerKind::Unsupported
        );
        // An automation whose action has not synced yet still runs — the
        // launch re-fetches the row; only the display name degrades.
        let orphan = triggered_actions(&rows, &[], "cli-1");
        assert_eq!(orphan[0].name, "");

        // The fingerprint hashes the trigger VALUE — key order (Electric's
        // jsonb round-trip) must not read as an edit, or the GUI and the CLI
        // would re-seed each other's state forever.
        let reordered = automation_row(
            "auto-mine",
            "act-1",
            "cli-1",
            true,
            serde_json::json!({"minuteOfDay": 420, "interval": "daily", "kind": "schedule"}),
        );
        assert_eq!(
            triggered_actions(&[reordered], &actions, "cli-1")[0]
                .triggered
                .fingerprint,
            mine[0].triggered.fingerprint
        );
    }

    /// EXP-583: the pins ride from the row to the launch options.
    #[test]
    fn triggered_actions_carry_the_launch_pins() {
        let mut row = automation_row("auto-1", "act-1", "cli-1", true, daily(420));
        row.agent = Some("codex".to_string());
        row.model = Some("gpt-5.1-codex".to_string());
        let resolved = triggered_actions(&[row], &[action_row("act-1")], "cli-1");
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

    /// The cap is hand-synced with the desktop's `steer_wiring` vec — the
    /// automation host must advertise itself or the web pickers hide it.
    #[test]
    fn action_caps_advertise_automations() {
        assert!(ACTION_CAPS.contains(&"automations"));
        let caps = device_caps(&advert(&["claude"]));
        assert!(caps.contains(&"automations".to_string()));
        // Nothing runnable = nothing to run an automation with.
        assert!(!device_caps(&advert(&[])).contains(&"automations".to_string()));
        // EXP-615: the same for chat — remote Chat starts gate on this cap,
        // so an agent-less machine must never advertise it.
        assert!(ACTION_CAPS.contains(&"chat"));
        assert!(caps.contains(&"chat".to_string()));
        assert!(!device_caps(&advert(&[])).contains(&"chat".to_string()));
    }

    /// EXP-484: signing IN is exactly what a machine with no runnable agent
    /// needs, so `agent-login` is a BUILD cap — it rides even when nothing
    /// is signed in, alongside the desktop's hand-synced `steer_wiring` vec.
    #[test]
    fn device_caps_include_agent_login_without_runnable_agents() {
        assert!(DEVICE_CAPS.contains(&"agent-login"));
        assert!(!ACTION_CAPS.contains(&"agent-login"));
        let signed_out = device_caps(&advert(&[]));
        assert!(signed_out.contains(&"agent-login".to_string()));
        assert!(device_caps(&advert(&["claude"])).contains(&"agent-login".to_string()));
    }

    /// EXP-679: `agent-start` asserts this build understands a start frame's
    /// `started_reason` — a PROTOCOL property of the binary, not of what it
    /// can run, so it rides with the build caps and the server may gate an
    /// agent-parented start on it.
    #[test]
    fn device_caps_include_agent_start_as_a_build_cap() {
        assert!(DEVICE_CAPS.contains(&"agent-start"));
        assert!(!ACTION_CAPS.contains(&"agent-start"));
        assert!(device_caps(&advert(&[])).contains(&"agent-start".to_string()));
        assert!(device_caps(&advert(&["claude"])).contains(&"agent-start".to_string()));
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
