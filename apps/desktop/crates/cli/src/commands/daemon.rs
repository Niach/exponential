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
const DOCTOR_RECHECK: Duration = Duration::from_secs(5 * 60);
/// EXP-414: a changed agent advertisement is only ACTED on once a second
/// probe agrees ([`advert_transition`]) — this is the shortened recheck that
/// confirms (or clears) a pending change, so a real change still converges
/// in ~DOCTOR_RECHECK + this instead of two full periods.
const ADVERT_CONFIRM_RECHECK: Duration = Duration::from_secs(30);
/// EXP-481 adds `resume` (start_session honors the resume flag),
/// `worktrees` (inventory reporting + remove/prune commands) and
/// `launch-defaults` (server-authoritative defaults convergence + the
/// `check_in` nudge frame). EXP-490 split them off [`ACTION_CAPS`]: these
/// three are BUILD capabilities and ride even with zero runnable agents —
/// dropping them made the server skip the `check_in` nudge
/// (`deviceUnderstandsCheckIn`), so every remote command/defaults edit
/// waited out the 30s heartbeat.
pub const DEVICE_CAPS: [&str; 3] = ["resume", "worktrees", "launch-defaults"];

/// The action-run capabilities — advertised only while at least one agent is
/// RUNNABLE (EXP-409: a machine whose only agents are signed out cannot run
/// actions either).
pub const ACTION_CAPS: [&str; 3] = ["actions", "action-inputs", "fix-conflicts"];

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

/// The running daemon's pid, liveness-checked.
pub fn daemon_pid(data_dir: &Path) -> Option<u32> {
    let raw = std::fs::read_to_string(pidfile(data_dir)).ok()?;
    let pid: u32 = raw.trim().parse().ok()?;
    let alive = unsafe { libc::kill(pid as i32, 0) } == 0;
    alive.then_some(pid)
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
#[derive(Clone, Copy, Debug)]
enum UpdateTrigger {
    /// The settings-gated periodic check came due.
    Scheduled,
    /// The web "Update" button (heartbeat `updateRequested`) — acts even
    /// with auto-update off, and consumes the request either way.
    Requested,
}

/// One live session the daemon supervises (the desktop's `LocalSessions`).
struct LiveSession {
    issue_id: Option<String>,
    branch: String,
    is_fix_run: bool,
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
    // EXP-481: the serialized device-state worker (defaults convergence,
    // worktree commands, inventory reports) + the relay check_in flag that
    // forces an immediate heartbeat (the beat IS the work pull).
    let device_worker = spawn_device_worker(Arc::clone(&ctx), Arc::clone(&sessions), device_id.clone());
    let check_in = Arc::new(AtomicBool::new(false));

    let mut advertised = probe_agents(&ctx);
    // EXP-414: a failed register (network not up yet at boot) is retried on
    // the heartbeat cadence — otherwise the registry row goes stale (old
    // version/agents, a never-cleared update request) until the next restart.
    let mut registered_ok =
        register_device(&ctx, &device_id, &device_label, &advertised, &device_worker);
    device_worker.send(DeviceWork::ReportWorktrees).ok();
    // `register` only SEEDS the label (it never stomps a rename); an
    // explicit --label is an intentional write and goes through `rename`.
    if let Some(label) = &explicit_label {
        if let Err(err) = api::devices::rename(&ctx.trpc, &device_id, label) {
            log::debug!("devices.rename for --label failed: {err}");
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
    // EXP-414: an advertisement change observed by ONE probe, awaiting a
    // second agreeing probe before it tears the control channel down.
    let mut pending_advert: Option<coding::AgentAdvertisement> = None;
    // EXP-411: the live-session count last reported over the heartbeat. A
    // change forces an off-cadence beat so a session starting or ending
    // converges in ~1s (and a restarted daemon corrects a stale count on its
    // first tick) instead of up to a full heartbeat interval.
    let mut reported_sessions: Option<usize> = None;
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

        lock_sessions(&sessions).retain(|live| !live.session.is_done());

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
            match api::devices::heartbeat(
                &ctx.trpc,
                &device_id,
                live_now as u32,
                synced_at.as_deref(),
            ) {
                Ok(result) => {
                    // Row removed in the UI while we run, or an earlier
                    // register never landed (EXP-414) — re-register.
                    if !result.ok || !registered_ok {
                        registered_ok = register_device(
                            &ctx, &device_id, &device_label, &advertised, &device_worker,
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
                Err(err) => log::debug!("devices.heartbeat failed: {err}"),
            }
        }
        // Session start/end changes the inventory's busy flags.
        if session_change {
            device_worker.send(DeviceWork::ReportWorktrees).ok();
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
                        if matches!(trigger, UpdateTrigger::Requested) {
                            // Consume the web request even when there was
                            // nothing to install.
                            registered_ok = register_device(
                                &ctx, &device_id, &device_label, &advertised, &device_worker,
                            );
                        }
                    }
                    Err(err) => {
                        log::warn!("update failed: {err:#}");
                        if matches!(trigger, UpdateTrigger::Requested) {
                            registered_ok = register_device(
                                &ctx, &device_id, &device_label, &advertised, &device_worker,
                            );
                        }
                    }
                }
            }
        }

        // Re-advertise on toolchain changes (the desktop's
        // `restart_control_channel_if_needed`): installing the first agent
        // brings remote start online without a restart; removing the last
        // hangs up. Sign-in state rides the same probe (EXP-409): logging
        // into claude over ssh flips the machine runnable without a restart.
        // EXP-414: acted on only once TWO consecutive probes agree — the
        // stop + re-dial is a real presence gap, and a single flaky auth
        // probe was flapping the machine offline every 5 minutes.
        let doctor_due = if pending_advert.is_some() {
            ADVERT_CONFIRM_RECHECK
        } else {
            DOCTOR_RECHECK
        };
        if last_doctor.elapsed() >= doctor_due {
            last_doctor = Instant::now();
            // EXP-481: the slow cadence also picks up hand-edited
            // settings.json defaults and re-checks the inventory.
            device_worker.send(DeviceWork::ReconcileLocal).ok();
            device_worker.send(DeviceWork::ReportWorktrees).ok();
            let agents = probe_agents(&ctx);
            match advert_transition(&advertised, &agents, &mut pending_advert) {
                AdvertStep::Keep => {}
                AdvertStep::AwaitConfirmation => log::info!(
                    "agent advertisement change observed ({advertised:?} -> {agents:?}) — awaiting confirmation"
                ),
                AdvertStep::Apply => {
                    log::info!("agent advertisement changed: {advertised:?} -> {agents:?}");
                    advertised = agents;
                    if let Some(handle) = control.take() {
                        handle.stop();
                    }
                    control = runtime.as_ref().and_then(|runtime| {
                        dial_control(
                            runtime, &ctx, &device_id, &device_label, &advertised, &inbox_tx,
                            &check_in,
                        )
                    });
                    registered_ok = register_device(
                        &ctx, &device_id, &device_label, &advertised, &device_worker,
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

fn probe_agents(ctx: &Ctx) -> coding::AgentAdvertisement {
    let settings = coding::Settings::load(&coding::Settings::default_path(&ctx.data_dir));
    coding::run_doctor(&settings).agent_advertisement(&settings)
}

/// EXP-437: the wire form of the doctor advertisement's launch defaults —
/// `None` when no agent is runnable. Twin of `steer_wiring`'s copy (the
/// `coding` and `steer` crates deliberately don't depend on each other).
fn launch_defaults_frame(
    advertisement: &coding::AgentAdvertisement,
) -> Option<steer::LaunchDefaults> {
    if advertisement.agents.is_empty() {
        return None;
    }
    Some(steer::LaunchDefaults {
        default_agent: Some(advertisement.default_agent.clone()),
        agents: advertisement
            .launch_defaults
            .iter()
            .map(|(agent, defaults)| {
                (
                    agent.clone(),
                    steer::AgentLaunchDefaults {
                        model: defaults.model.clone(),
                        effort: defaults.effort.clone(),
                        ultracode: defaults.ultracode,
                        plan_mode: defaults.plan_mode,
                        skip_permissions: defaults.skip_permissions,
                    },
                )
            })
            .collect(),
    })
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
    device_worker: &flume::Sender<DeviceWork>,
) -> bool {
    let caps = device_caps(advertised);
    // EXP-481: the local defaults ride the register as a FIRST-EVER seed
    // (the server applies them only while its column is NULL); the response
    // carries the authoritative copy either way, reconciled off-loop.
    let settings = coding::Settings::load(&coding::Settings::default_path(&ctx.data_dir));
    let launch_defaults = serde_json::to_value(coding::defaults_wire(&settings))
        .expect("defaults serialize cannot fail");
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
    let caps = device_caps(advertised);
    let device = DeviceIdentity {
        device_id: device_id.to_string(),
        device_label: device_label.to_string(),
        agents: advertised.agents.clone(),
        unauthed_agents: advertised.unauthed_agents.clone(),
        caps,
        launch_defaults: launch_defaults_frame(advertised),
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
        start.skip_permissions,
    );
    let origin = coding::LaunchOrigin::Relay {
        device_id: device_id.to_string(),
        claimant: ctx.account.id.clone(),
        started_by: start.started_by.clone(),
    };

    // Errors and refusals are logged, never acked — the remote client
    // observes success purely via the synced `coding_sessions` row
    // appearing (desktop parity).
    let outcome = match start.subject.clone() {
        RemoteStartSubject::Issue(issue_id) => remote_issue_start(
            ctx, sidecars, runtime, sessions, personal_key, options, origin, issue_id,
            // EXP-481: honor the remote resume flag — the launcher's marker
            // gate degrades a missing/foreign worktree to a fresh session.
            start.resume.unwrap_or(false),
        ),
        RemoteStartSubject::Batch { issue_ids, team_id, repo } => remote_batch_start(
            ctx, sidecars, runtime, sessions, personal_key, options, origin, issue_ids, team_id, repo,
        ),
        RemoteStartSubject::Action { action_id, team_id, repo, inputs, .. } => remote_action_start(
            ctx, sidecars, runtime, sessions, personal_key, options, origin, action_id, team_id, repo, inputs,
        ),
    };
    if let Err(err) = outcome {
        log::warn!("remote start failed: {err:#}");
    }
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
    if issue_is_coding_here(sessions, &issue_id) {
        log::info!("remote start for {issue_id} ignored — already coding this issue");
        return Ok(());
    }
    // REV2-24 cross-device guard (desktop parity): one session per issue,
    // wherever it runs. Best-effort — an older server without the probe
    // must not block the start.
    if let Ok(Some(live)) = api::coding_sessions::live_for_issue(&ctx.trpc, &issue_id) {
        log::info!(
            "remote start for {issue_id} ignored — live session on {} (one session per issue)",
            live.device_label.as_deref().unwrap_or("another device")
        );
        return Ok(());
    }
    let fetched = api::issues::issues_get(&ctx.trpc, &issue_id).context("resolve the issue")?;
    let issue = fetched.issue;
    let request =
        launch::issue_launch_request(&issue, options, origin, start_resume);
    let mut seeds = HashMap::new();
    seeds.insert(issue.id.clone(), launch::issue_seed(&issue));
    let deps = launch::coding_deps(ctx, seeds);
    let prepared = coding::prepare_with_hooks(
        &PrepareRequest::Issue(request),
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
    let deps = launch::coding_deps(ctx, seeds);
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

    let deps = launch::coding_deps(ctx, HashMap::new());
    let prepared = coding::prepare_with_hooks(
        &PrepareRequest::Action(request),
        &deps,
        sidecars.hook_setup().as_ref(),
        sidecars.observer_setup().as_ref(),
    )
    .map_err(|err| anyhow::anyhow!("{err}"))?;
    spawn_prepared(ctx, sidecars, runtime, sessions, personal_key, prepared, None, is_fix_run)
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
    let session = session_host::launch(&env, prepared, false, issue_id.clone())?;
    log::info!(
        "session {} started ({}, branch {})",
        session.session_id,
        session.issue_identifier,
        session.branch
    );
    lock_sessions(sessions).push(LiveSession {
        issue_id,
        branch: session.branch.clone(),
        is_fix_run,
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
}

fn spawn_device_worker(
    ctx: Arc<Ctx>,
    sessions: Sessions,
    device_id: String,
) -> flume::Sender<DeviceWork> {
    let (tx, rx) = flume::unbounded::<DeviceWork>();
    std::thread::spawn(move || {
        let mut last_inventory_fp: Option<u64> = None;
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
                        run_device_command(&ctx, &sessions, &command);
                    }
                    report_worktrees(&ctx, &sessions, &device_id, &mut last_inventory_fp);
                }
                DeviceWork::ReportWorktrees => {
                    report_worktrees(&ctx, &sessions, &device_id, &mut last_inventory_fp);
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
fn run_device_command(ctx: &Ctx, sessions: &Sessions, command: &api::devices::PendingCommand) {
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
        "worktree_prune" => run_prune(&settings, &repos_root, held),
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
    held: std::collections::HashSet<String>,
) -> (bool, String) {
    let policy =
        coding::conservative_prune_policy(&settings.branch_prefix, held, Vec::new());
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
