//! App-side wiring for the remote-steer subsystem (masterplan-v3 §08) — the
//! ONE place that ties the `steer` crate, the `sync::kill_watch` own-row
//! kill-switch, and the running `coding` sessions together. `steer` depends on
//! neither `sync` nor `ui`, and `coding` depends on neither `steer` nor `ui`
//! (§3.1); this module is the intended meeting point ("the app/ui layer wires
//! both", `steer::lib` docs).
//!
//! The three seams, stated exactly:
//!
//! 1. **Control channel — starts with the app session.** [`install`] (from the
//!    app bootstrap, `main.rs`) creates the single [`steer::SteerRuntime`],
//!    installs the [`sync::KillWatch`], and stands up the remote-`start_session`
//!    inbox + its foreground drain. [`start_control_channel`] (from
//!    `session::connect_account`, once per signed-in account) dials the
//!    device-presence socket; [`stop_control_channel`] (from
//!    `session::sign_out_active`) tears it down. A relay `start_session` frame
//!    lands on the socket → the inbox → [`handle_remote_start`] → the §7
//!    launcher on a shell window (the SAME `coding_flow` path the button
//!    uses, `LaunchOrigin::Relay`).
//!
//! 2. **Publisher — attaches on coding-session launch.**
//!    [`attach_publisher`] is the single call `coding_flow::spawn_into_window`
//!    makes right after `coding` reports `LaunchOutcome::Spawned`. It mints a
//!    publisher ticket over tRPC (never signed locally), wires the live
//!    terminal's Send+Sync handles (`Terminal::writer()` for remote input and
//!    answer injection, `Terminal::term()` for the grid the emitter watches),
//!    starts the §P7 activity emitter with this session's hook-sidecar stream,
//!    and registers the session for the kill-switch. Best-effort: a disabled/
//!    unreachable relay is a no-op (the session runs fine locally).
//!
//!    EXP-249 also puts the **claude hooks sidecar** here: [`install`] starts
//!    ONE loopback [`steer::HookServer`] per process, [`hook_setup`] hands its
//!    port/token/settings to `coding::prepare_with_hooks` at every launch site,
//!    and a router thread fans each delivery to the session whose worktree it
//!    came from.
//!
//! 3. **Kill-switch — the own-row Electric watch (§8.8).** Every published
//!    session is registered with the [`sync::KillWatch`]; when its
//!    `coding_sessions` row flips to `ended` (a `steer.killSession` DB write
//!    that reaches us over sync even when the relay is dead), the callback
//!    kills the `claude` child and stops the publisher — the only kill path
//!    that survives a dead relay.
//!
//! Cross-thread discipline: the publisher task runs on the steer tokio runtime,
//! so its hooks must be `Send + Sync`. Input-inject operates on `Send + Sync`
//! `Arc` handles directly (the shared PTY writer + the `TermHandle`); the rest
//! (presence, error, relay-kill) marshal onto the gpui foreground through a
//! per-session [`flume`] channel drained by a foreground task, because they
//! touch the gpui-held `Terminal` / registry.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use gpui::{App, AppContext as _, Entity, Global, WeakEntity};
use terminal::{TabId, TerminalManager};

use coding::{
    prepare_with_hooks, BatchIssueSpec, BatchLaunchRequest, CodingAgent, LaunchOptions,
    LaunchOrigin, Prepared, PrepareRequest, RepoGroup, ResumeRunRequest,
};
use steer::publisher::pty_writer_input_hook;
use steer::{
    spawn_activity_emitter, spawn_control_channel, AnswerLink, CommandLink, ControlApi,
    ControlChannelHandle, DeviceIdentity, EmitterConfig, HookEvent, HookServer, PublishSpec,
    PublisherHandle, PublisherHooks, PublisherTickets, Steering, SteerRuntime, TrpcControlApi,
    TrpcPublisherTickets,
};
use sync::{KillWatch, Store};

use crate::coding_flow;
use crate::queries;
use crate::session::AuthContext;

// ---------------------------------------------------------------------------
// Process globals (created by `install`)
// ---------------------------------------------------------------------------

/// The single steer tokio runtime (§3.5) — shared by the control channel and
/// every publisher. Absent ⇒ steer failed to init and stays off gracefully.
struct SteerRuntimeGlobal(Arc<SteerRuntime>);
impl Global for SteerRuntimeGlobal {}

/// The §8.8 own-row kill-switch entity.
struct KillWatchGlobal(Entity<KillWatch>);
impl Global for KillWatchGlobal {}

/// Foreground inbox for relay `start_session` frames (the socket callback runs
/// on the steer runtime; this hands the start request to the gpui foreground).
struct RemoteStartGlobal(flume::Sender<steer::RemoteStart>);
impl Global for RemoteStartGlobal {}

/// The ONE steer tokio runtime, once [`install`] has stood it up. `None` on a
/// build where it failed to start — every steer role (publisher, control
/// channel, and the EXP-696 viewer) is then simply off.
pub(crate) fn runtime(cx: &App) -> Option<Arc<SteerRuntime>> {
    cx.try_global::<SteerRuntimeGlobal>().map(|g| g.0.clone())
}

/// Stand up the steer subsystem. Called ONCE from the app bootstrap, after the
/// `Store` + `AuthContext` globals are set and before the session bootstrap
/// connects an account (so the control-channel infra is ready).
pub fn install(cx: &mut App) {
    if cx.has_global::<SteerRuntimeGlobal>() {
        return;
    }
    match SteerRuntime::new() {
        Ok(rt) => cx.set_global(SteerRuntimeGlobal(rt)),
        Err(err) => {
            log::warn!("steer: runtime init failed — remote steer disabled ({err})");
            return;
        }
    }

    // §8.8 own-row Electric kill-switch: install the watch over the shared
    // `coding_sessions` collection.
    let store = Store::global(cx).clone();
    let kill_watch = KillWatch::install(&store, cx);
    cx.set_global(KillWatchGlobal(kill_watch));

    // Lazily-created entity globals — materialize now so later access never
    // races the first coding session.
    let _ = PublisherRegistry::global(cx);
    let _ = ControlChannels::global(cx);

    // EXP-249: the claude hooks sidecar — one loopback server per process,
    // its port/token handed to every launch through `hook_setup`.
    match HookSidecar::start() {
        Some(sidecar) => cx.set_global(HookSidecarGlobal(sidecar)),
        None => log::warn!("steer: hooks sidecar unavailable — grid-only detection"),
    }

    // EXP-383: the pi observer sidecar — same one-server-per-process shape,
    // its port/token handed to every launch through `observer_setup`.
    match steer::pi_observer::ObserverServer::start() {
        Ok(server) => cx.set_global(PiObserverGlobal(Arc::new(server))),
        Err(err) => log::warn!("steer: pi observer sidecar failed to bind: {err}"),
    }

    // §8.3 #4: relay `start_session` → foreground launcher.
    let (tx, rx) = flume::unbounded::<steer::RemoteStart>();
    cx.set_global(RemoteStartGlobal(tx));
    cx.spawn(async move |cx| {
        while let Ok(start) = rx.recv_async().await {
            cx.update(|cx| handle_remote_start(start, cx));
        }
    })
    .detach();
}

// ---------------------------------------------------------------------------
// The claude hooks sidecar (EXP-249) — one server, per-session routing
// ---------------------------------------------------------------------------

/// One session's subscription to the sidecar.
struct HookSubscriber {
    worktree: PathBuf,
    tx: flume::Sender<HookEvent>,
    /// claude session ids already routed here — the tie-breaker when two
    /// sessions share a cwd (two action runs on the same trunk clone).
    bound: HashSet<String>,
}

/// The process-wide hooks sidecar: the loopback server, the `HookSetup` every
/// launch site passes to `coding::prepare_with_hooks`, and the router that
/// fans each delivery to the session it came from (by `cwd`, then pinned by
/// claude's own session id).
struct HookSidecar {
    setup: coding::HookSetup,
    subscribers: Arc<Mutex<Vec<HookSubscriber>>>,
    /// Alive for the process — dropping it stops the accept loop.
    _server: HookServer,
}

struct HookSidecarGlobal(Arc<HookSidecar>);
impl Global for HookSidecarGlobal {}

impl HookSidecar {
    fn start() -> Option<Arc<Self>> {
        let server = match HookServer::start() {
            Ok(server) => server,
            Err(err) => {
                log::warn!(
                    "steer: hooks sidecar failed to bind: {err} — every session degrades to \
                     grid-only detection (no question identity, no transcript pin)"
                );
                return None;
            }
        };
        let setup = coding::HookSetup {
            port: server.port(),
            token: server.token().to_string(),
            settings_json: server.settings_json(),
        };
        let subscribers: Arc<Mutex<Vec<HookSubscriber>>> = Arc::new(Mutex::new(Vec::new()));
        let events = server.events().clone();
        let routed = Arc::clone(&subscribers);
        std::thread::Builder::new()
            .name("exp-hook-router".to_string())
            .spawn(move || {
                while let Ok(event) = events.recv() {
                    route_hook_event(&routed, event);
                }
            })
            .ok()?;
        Some(Arc::new(Self {
            setup,
            subscribers,
            _server: server,
        }))
    }

    /// Subscribe a session's worktree; the receiver goes to its emitter and
    /// dropping it unsubscribes on the next delivery. `session_id` (EXP-443)
    /// is the launcher-minted `--session-id`: pre-seeding `bound` with it
    /// makes rule 1 of [`route_hook_event`] authoritative from the first
    /// delivery — no cwd guess, no insertion-order race with a same-cwd
    /// subscriber.
    fn subscribe(&self, worktree: &Path, session_id: Option<&str>) -> flume::Receiver<HookEvent> {
        let (tx, rx) = flume::unbounded();
        let mut subscribers = match self.subscribers.lock() {
            Ok(subscribers) => subscribers,
            Err(poisoned) => poisoned.into_inner(),
        };
        subscribers.retain(|subscriber| !subscriber.tx.is_disconnected());
        subscribers.push(HookSubscriber {
            worktree: canonical(worktree),
            tx,
            bound: session_id
                .map(|id| HashSet::from([id.to_string()]))
                .unwrap_or_default(),
        });
        rx
    }
}

/// `std::fs::canonicalize` or the path as given — the hook payload's `cwd` and
/// our worktree must compare equal through symlinked temp/home dirs.
fn canonical(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

/// Deliver one hook event to at most ONE session: a claude session id we have
/// already seen wins outright, otherwise the cwd picks the session (preferring
/// one with no bound id yet, so two runs sharing a trunk clone split instead
/// of piling onto the first).
///
/// EXP-443: sessions launch with their `bound` set pre-seeded by the minted
/// `--session-id`, so rule 1 is authoritative from the first delivery and
/// the cwd tie-break no longer races two same-cwd runs. The cwd fallback
/// stays for the ids a pre-seed cannot know: a `/clear`-minted rotation and
/// pre-EXP-443 resumes.
fn route_hook_event(subscribers: &Arc<Mutex<Vec<HookSubscriber>>>, event: HookEvent) {
    let mut subscribers = match subscribers.lock() {
        Ok(subscribers) => subscribers,
        Err(poisoned) => poisoned.into_inner(),
    };
    subscribers.retain(|subscriber| !subscriber.tx.is_disconnected());
    let session_id = event.context.session_id.clone();
    let target = session_id
        .as_ref()
        .and_then(|id| {
            subscribers
                .iter()
                .position(|subscriber| subscriber.bound.contains(id))
        })
        .or_else(|| {
            let cwd = canonical(Path::new(event.context.cwd.as_deref()?));
            subscribers
                .iter()
                .position(|subscriber| subscriber.worktree == cwd && subscriber.bound.is_empty())
                .or_else(|| {
                    subscribers
                        .iter()
                        .position(|subscriber| subscriber.worktree == cwd)
                })
        });
    let Some(target) = target else {
        log::debug!("steer: hook event with no matching session, dropped");
        return;
    };
    if let Some(id) = session_id {
        subscribers[target].bound.insert(id);
    }
    let _ = subscribers[target].tx.send(event);
}

/// The sidecar wiring every launch site passes to
/// `coding::prepare_with_hooks` (EXP-249). `None` = no sidecar (bind failed,
/// or steer never installed) — the launcher then writes no `--settings` file
/// and the session runs on grid-only detection.
pub fn hook_setup(cx: &App) -> Option<coding::HookSetup> {
    cx.try_global::<HookSidecarGlobal>()
        .map(|global| global.0.setup.clone())
}

// ---------------------------------------------------------------------------
// The pi observer sidecar (EXP-383) — one server, per-worktree routing
// ---------------------------------------------------------------------------

/// The process-wide pi observer server ([`steer::pi_observer`]). Routing to
/// sessions lives inside the server itself (by canonicalized worktree), so
/// unlike the hooks sidecar there is no router thread here.
struct PiObserverGlobal(Arc<steer::pi_observer::ObserverServer>);
impl Global for PiObserverGlobal {}

/// The observer wiring every launch site passes to
/// `coding::prepare_with_hooks` (EXP-383). `None` = no sidecar — a pi
/// session then runs with the observer extension inert (diffs-only feed).
pub fn observer_setup(cx: &App) -> Option<coding::ObserverSetup> {
    cx.try_global::<PiObserverGlobal>()
        .map(|global| coding::ObserverSetup {
            port: global.0.port(),
            token: global.0.token().to_string(),
        })
}

// ---------------------------------------------------------------------------
// Control channel (§8.3) — per account, starts with the app session
// ---------------------------------------------------------------------------

/// Per-account [`ControlChannelHandle`]s (multi-window shares the one channel
/// per account — it is account-scoped, not window-scoped, §8.3).
#[derive(Default)]
struct ControlChannels {
    by_account: HashMap<String, ControlChannelHandle>,
    /// The agent advertisement the account's presence currently carries —
    /// runnable + signed-out sets (EXP-409), including the DELIBERATE
    /// absence (nothing installed → offline, EXP-367).
    /// [`refresh_device_advertisement`] compares against it so a doctor
    /// refresh only re-registers (or re-dials) when the sets actually changed.
    advertised: HashMap<String, coding::AgentAdvertisement>,
}
struct ControlChannelsGlobal(Entity<ControlChannels>);
impl Global for ControlChannelsGlobal {}

impl ControlChannels {
    fn global(cx: &mut App) -> Entity<ControlChannels> {
        if let Some(g) = cx.try_global::<ControlChannelsGlobal>() {
            return g.0.clone();
        }
        let entity = cx.new(|_| ControlChannels::default());
        cx.set_global(ControlChannelsGlobal(entity.clone()));
        entity
    }

    fn global_ref(cx: &App) -> Option<Entity<ControlChannels>> {
        cx.try_global::<ControlChannelsGlobal>().map(|g| g.0.clone())
    }
}

/// Dial the device-presence control socket for `account` (§8.3). Called from
/// `session::connect_account` on every sign-in / warm-start. A no-op when the
/// steer runtime failed to init; the channel itself no-ops when `steer.config`
/// reports the relay disabled (an unconfigured instance is silent).
pub fn start_control_channel(account: &api::Account, cx: &mut App) {
    let Some(runtime) = runtime(cx) else {
        return;
    };
    let auth = AuthContext::global(cx).clone();
    let provider = auth.auth.token_provider(&account.id);
    let trpc = Arc::new(api::TrpcClient::new(&account.instance_url, provider));

    // EXP-201: advertise which agent CLIs this machine can actually run —
    // remote Start-coding pickers only offer these. Probed via the coding
    // doctor (blocking `--version` spawns) on the BACKGROUND executor, then
    // the channel starts on the foreground with the result. A settings /
    // toolchain change re-advertises via `refresh_device_advertisement`
    // (every doctor refresh), besides the sign-in / account switch / relay
    // reconnect cycles that re-run this whole function.
    let settings = coding_flow::CodingHub::global(cx).read(cx).settings.clone();
    let settings2 = settings.clone();
    let device_id = steer::persistent_device_id(&auth.data_dir);
    let device_label = api::users::hostname();
    let inbox = cx.global::<RemoteStartGlobal>().0.clone();
    let check_in = crate::device_sync::check_in_flag(cx);
    let account_id = account.id.clone();
    cx.spawn(async move |cx| {
        // EXP-484: the REPORT is kept, not just the advertisement it
        // derives — `devices.register` also carries the per-agent accounts.
        let report: coding::DoctorReport = cx
            .background_executor()
            .spawn(async move { coding::run_doctor(&settings) })
            .await;
        let advertisement: coding::AgentAdvertisement = report.agent_advertisement(&settings2);
        let caps = device_caps(&advertisement);
        // EXP-403: record this machine in the per-user devices registry so the
        // agents UI can show it with an offline "last seen" state (relay
        // presence alone empties on every relay restart). Best-effort — an
        // older server without the devices router must never break
        // control-channel start — and registered even with no agents so the
        // UI can explain WHY the machine is not startable. EXP-485: this row
        // is now the ONLY path the agent lists and launch defaults take to the
        // server; the online frame carries neither.
        register_device(
            Arc::clone(&trpc),
            device_id.clone(),
            device_label.clone(),
            &advertisement,
            &caps,
            &settings2,
            Some(&report),
            &cx.background_executor(),
        );
        let _ = cx.update(|cx| {
            // The probe raced a sign-out/switch: starting a socket for a
            // no-longer-active account would leak it past its stop call.
            if queries::active_account(cx).map(|account| account.id)
                != Some(account_id.clone())
            {
                return;
            }
            // EXP-367: NO agent CLI installed at all → this device cannot
            // run anything remotely, so it goes fully OFFLINE for remote
            // start: don't dial at all and hang up any live socket from
            // before the last agent vanished. An installed-but-signed-out
            // agent (EXP-409) still dials, so the machine list can say
            // "sign in" instead of showing the device offline. A later
            // doctor refresh re-dials via `refresh_device_advertisement`.
            if advertisement.nothing_installed() {
                let channels = ControlChannels::global(cx);
                channels.update(cx, |channels, _| {
                    if let Some(previous) = channels.by_account.remove(&account_id) {
                        previous.stop();
                    }
                    channels
                        .advertised
                        .insert(account_id, coding::AgentAdvertisement::default());
                });
                return;
            }
            // EXP-672: the control socket announces presence and nothing
            // else — `devices.register` above owns the label, the agent
            // lists, the launch defaults and the caps starts gate on.
            let device = DeviceIdentity {
                device_id,
                device_label,
            };
            let on_start: steer::control_channel::StartSessionFn = Arc::new(move |start| {
                let _ = inbox.send(start);
            });
            // Non-blocking by contract: flip the flag — the device-sync
            // loop's next 1s tick beats immediately.
            let on_check_in: steer::control_channel::CheckInFn = Arc::new(move || {
                check_in.store(true, std::sync::atomic::Ordering::SeqCst);
            });
            let control_api: Arc<dyn ControlApi> = Arc::new(TrpcControlApi(trpc));
            let handle =
                spawn_control_channel(&runtime, device, control_api, on_start, on_check_in);

            let channels = ControlChannels::global(cx);
            channels.update(cx, |channels, _| {
                channels.advertised.insert(account_id.clone(), advertisement);
                if let Some(previous) = channels.by_account.insert(account_id, handle) {
                    previous.stop(); // never accumulate two sockets for one account
                }
            });
        });
    })
    .detach();
}

/// Stop the control socket for `account_id` (§8.3) — from
/// `session::sign_out_active`.
pub fn stop_control_channel(account_id: &str, cx: &mut App) {
    if let Some(channels) = ControlChannels::global_ref(cx) {
        channels.update(cx, |channels, _| {
            if let Some(handle) = channels.by_account.remove(account_id) {
                handle.stop();
            }
            // Sign-out forgets the device advertisement — the next sign-in
            // starts from a clean probe.
            channels.advertised.remove(account_id);
        });
    }
}

/// Re-post this machine's advertisement (and, when the DIAL decision itself
/// flipped, re-dial or hang up the control socket) after a doctor refresh.
/// Called from `CodingHub::refresh_doctor` completion — the one choke point
/// every "Check tools" / settings save / onboarding recheck funnels through.
///
/// EXP-485: the online frame no longer carries the agent lists or the launch
/// defaults, so a toolchain/settings change only has to reach the devices
/// ROW — a plain `devices.register`, no presence gap. The socket is only torn
/// down or brought up when the dial decision itself changes, i.e. when
/// `nothing_installed()` flips (EXP-367: the last agent CLI vanishing takes
/// the device offline for remote start; installing the first brings it back).
pub fn refresh_device_advertisement(cx: &mut App) {
    let Some(account) = queries::active_account(cx) else {
        return;
    };
    let Some(hub) = coding_flow::CodingHub::global_ref(cx) else {
        return;
    };
    let Some(report) = hub.read(cx).doctor.report.clone() else {
        return;
    };
    let settings = hub.read(cx).settings.clone();
    // The advertisement embeds the per-agent launch defaults, so a Settings →
    // Agents edit (model/effort/toggles/default agent) re-registers just like
    // an install/uninstall does — the equality check covers both.
    let desired = report.agent_advertisement(&settings);
    let current = ControlChannels::global_ref(cx)
        .and_then(|channels| channels.read(cx).advertised.get(&account.id).cloned());
    if current.as_ref() == Some(&desired) {
        return;
    }
    // The dial/hang-up decision flipped → the full path (`start_control_channel`
    // re-probes on the background executor itself, so a disagreeing race
    // converges on the set it actually advertises, and it registers too).
    let dialing = current
        .as_ref()
        .is_none_or(|current| current.nothing_installed());
    if dialing != desired.nothing_installed() {
        start_control_channel(&account, cx);
        return;
    }
    // Otherwise: the socket stays exactly as it is; only the row changes.
    let Some(auth) = cx.try_global::<AuthContext>().cloned() else {
        return;
    };
    let provider = auth.auth.token_provider(&account.id);
    let trpc = Arc::new(api::TrpcClient::new(&account.instance_url, provider));
    let caps = device_caps(&desired);
    register_device(
        trpc,
        steer::persistent_device_id(&auth.data_dir),
        api::users::hostname(),
        &desired,
        &caps,
        &settings,
        Some(&report),
        &cx.background_executor(),
    );
    if let Some(channels) = ControlChannels::global_ref(cx) {
        channels.update(cx, |channels, _| {
            channels.advertised.insert(account.id.clone(), desired);
        });
    }
}

/// The caps this build advertises for a doctor snapshot (EXP-253/EXP-257: the
/// actions capabilities ride only while ANY agent is usable — action runs
/// stopped being Claude-only — plus `action-inputs` for builtin +
/// inputs-carrying starts and `fix-conflicts` for the EXP-259 builtin, whose
/// ids a pre-EXP-259 desktop would treat as real actions and fail to fetch).
/// EXP-481's `resume`/`worktrees`/`launch-defaults`, EXP-484's `agent-login`
/// and EXP-679's `agent-start` are BUILD capabilities and ride even with zero
/// runnable agents (a machine with nothing signed in is exactly the one a
/// remote Login button targets; `agent-start` is a property of the FRAME
/// parser, not of the agent list). Hand-synced with the CLI daemon's
/// `DEVICE_CAPS` + `ACTION_CAPS`.
fn device_caps(advertisement: &coding::AgentAdvertisement) -> Vec<String> {
    let mut caps: Vec<String> = [
        "resume",
        "worktrees",
        "launch-defaults",
        "agent-login",
        // EXP-679: this build reads `started_reason` off a `StartSession`
        // frame and forwards it into `codingSessions.start`, so an
        // agent-parented start (MCP `exponential_sessions_start`) lands
        // UNATTENDED. The server refuses one against a device lacking it.
        "agent-start",
    ]
    .iter()
    .map(|cap| cap.to_string())
    .collect();
    if !advertisement.agents.is_empty() {
        caps.extend(
            [
                "actions",
                "action-inputs",
                "fix-conflicts",
                // EXP-530: this build runs the automation host, so a trigger
                // may be BOUND to this device. Agent-gated with the rest — an
                // automation is an action run, and a machine with no runnable
                // agent could only fail every firing.
                "automations",
                // EXP-615: this build runs the hidden `builtin:chat` action.
                "chat",
                // EXP-637: this build can RESUME an ended run out of its own
                // run registry — since EXP-662 issue and batch sessions too.
                "resume-run",
            ]
            .iter()
            .map(|cap| cap.to_string()),
        );
    }
    caps
}

/// Best-effort `devices.register` on the background executor (EXP-403) — the
/// machine's row in the per-user registry, and since EXP-485 the ONLY path the
/// agent lists and launch defaults take to the server. An older server without
/// the devices router must never break control-channel start, so failures are
/// swallowed.
fn register_device(
    trpc: Arc<api::TrpcClient>,
    device_id: String,
    device_label: String,
    advertisement: &coding::AgentAdvertisement,
    caps: &[String],
    settings: &coding::Settings,
    report: Option<&coding::DoctorReport>,
    executor: &gpui::BackgroundExecutor,
) {
    let agents = advertisement.agents.clone();
    let unauthed_agents = advertisement.unauthed_agents.clone();
    let caps = caps.to_vec();
    // EXP-484: WHO each installed CLI is signed in as, straight off the
    // doctor probe that produced this advertisement. Skipped when nothing is
    // installed — the server then leaves the column untouched, so an older
    // build's re-register can never blank a row.
    let agent_accounts = report.and_then(|report| {
        let accounts = report.agent_accounts(&coding::agent_accounts::now_iso());
        (!accounts.is_empty())
            .then(|| serde_json::to_value(&accounts).ok())
            .flatten()
    });
    // EXP-481: the local defaults ride as a first-ever SEED (server-side no-op
    // once the column is set); the device-sync beat reconciles the response
    // copy within one interval, so it is deliberately dropped here.
    let launch_defaults = serde_json::to_value(coding::defaults_wire(settings))
        .expect("defaults serialize cannot fail");
    executor
        .spawn(async move {
            let _ = api::devices::register(
                &trpc,
                &api::devices::RegisterDevice {
                    device_id: &device_id,
                    label: &device_label,
                    kind: "desktop",
                    platform: Some(std::env::consts::OS),
                    agents: &agents,
                    unauthed_agents: &unauthed_agents,
                    caps: &caps,
                    launch_defaults: Some(&launch_defaults),
                    agent_accounts: agent_accounts.as_ref(),
                    version: Some(domain::client_version::current_version()),
                },
            );
        })
        .detach();
}

/// Relay `start_session` → the §7 launcher on a shell window. The SAME
/// sequence the Start-coding dialog runs (`coding::prepare` →
/// `spawn_into_window`), only the [`LaunchOrigin`] differs (§7.1: there is no
/// second, divergent remote-start implementation). Dispatches on the frame's
/// subject: a single issue (`build_launch` → `PrepareRequest::Issue`) or a
/// multi-issue batch (`PrepareRequest::Batch`, EXP-106).
fn handle_remote_start(start: steer::RemoteStart, cx: &mut App) {
    match start.subject.clone() {
        steer::RemoteStartSubject::Issue(issue_id) => remote_issue_start(issue_id, &start, cx),
        steer::RemoteStartSubject::Batch {
            issue_ids,
            team_id,
            repo,
        } => remote_batch_start(issue_ids, team_id, repo, &start, cx),
        steer::RemoteStartSubject::Action {
            action_id,
            team_id,
            repo,
            inputs,
            ..
        } => remote_action_start(action_id, team_id, repo, inputs, &start, cx),
        // EXP-637: resume an ended run out of the local run registry (EXP-662:
        // issue and batch sessions too) — no repo/inputs/options ride the
        // frame (the record has them), so this is the shortest arm of the four.
        steer::RemoteStartSubject::Resume { session_id } => crate::action_run::resume_run(
            session_id,
            None,
            true,
            relay_origin(cx, start.started_by.clone(), start.started_reason.clone()),
            cx,
        ),
    }
}

/// EXP-505: in-flight remote ACTION start reservations — the GUI twin of the
/// CLI daemon's REV-9 `StartReservations`. Starts are never acked, and
/// `steer.startSession` gives up after 3s (REV-34 `relay_timeout`) even when
/// the frame WAS delivered — so the requester retries, and the duplicate
/// frame would run the same action twice: unlike issue starts (guarded by
/// `LocalSessions` + the synced-row probe in `remote_issue_start`), action
/// runs have no natural dedup key — concurrent runs of one action are a
/// FEATURE locally. The claim covers exactly the blind window: taken on the
/// frame's arrival, keyed on the action id, and released only when the launch
/// pipeline finishes — by which point `spawn_into_window` has registered the
/// session and its synced row tells the remote client the start took. Local
/// (dialog) starts never claim: the user watching the dialog is the dedup.
#[derive(Clone, Default)]
pub(crate) struct StartReservations(Arc<Mutex<HashSet<String>>>);

impl StartReservations {
    /// All-or-nothing claim: `None` when the key is already held by an
    /// in-flight start (the caller drops the duplicate frame).
    pub(crate) fn claim(&self, key: String) -> Option<ReservationGuard> {
        let mut held = match self.0.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        if !held.insert(key.clone()) {
            return None;
        }
        Some(ReservationGuard {
            held: Arc::clone(&self.0),
            key,
        })
    }
}

/// Releases its key on drop — every early return and failure path in the
/// action-run pipeline frees the claim (a poisoned-lock release recovers via
/// `into_inner`), so a failed start never wedges its action until restart.
pub(crate) struct ReservationGuard {
    held: Arc<Mutex<HashSet<String>>>,
    key: String,
}

impl Drop for ReservationGuard {
    fn drop(&mut self) {
        let mut held = match self.held.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        held.remove(&self.key);
    }
}

/// The process-wide claim set. EXP-530: the automation host claims through
/// the SAME set as the relay path — a scheduled firing and a remote start of
/// one action must never both launch.
pub(crate) fn action_start_reservations() -> &'static StartReservations {
    static RESERVATIONS: std::sync::OnceLock<StartReservations> = std::sync::OnceLock::new();
    RESERVATIONS.get_or_init(StartReservations::default)
}

/// Relay ACTION start (§08 / EXP-253): runs directly — EXP-268 removed the
/// per-device trust gate (actions are team-owner-authored content), so an
/// unattended desktop just launches. The frame's server-resolved repo group
/// + input values ride through (the desktop syncs no repositories); the
/// fresh body comes from the runner's own `actions.get`. EXP-257: the
/// FULL option set is honored with the same per-agent normalization as an
/// issue start (the server already validated the vocabulary).
fn remote_action_start(
    action_id: String,
    team_id: String,
    repo: Option<steer::StartRepoGroup>,
    inputs: Vec<steer::StartInput>,
    start: &steer::RemoteStart,
    cx: &mut App,
) {
    // EXP-505: claim the frame's action id before launching — the only dedup
    // a duplicate delivery (REV-34 relay_timeout + requester retry) can hit
    // while the first start is still fetching/preparing. NOT a concurrency
    // limit: once a run registers, its claim is gone and further starts run
    // alongside it (batch precedent — concurrent repo-backed runs share the
    // trunk cwd exactly like two shell tabs).
    let Some(reservation) = action_start_reservations().claim(format!("action:{action_id}"))
    else {
        log::info!(
            "steer: remote action start for {action_id} ignored — a start holding it is already in flight"
        );
        return;
    };
    let settings = coding_flow::CodingHub::global(cx).read(cx).settings.clone();
    let options = LaunchOptions::remote(
        &settings,
        start.agent.as_deref(),
        start.model.as_deref(),
        start.effort.as_deref(),
        start.ultracode,
        start.plan_mode,
    );
    let repo_group = repo.map(|repo| RepoGroup {
        repository_id: repo.repository_id,
        full_name: repo.full_name,
        default_branch: repo.default_branch,
    });
    // Frame inputs → prompt values. A thinned entry degrades per-field:
    // label falls back to the key, type to `text` (both presentation-only —
    // the VALUE the server resolved is what the run consumes).
    let inputs: Vec<coding::ActionInputValue> = inputs
        .into_iter()
        .map(|input| coding::ActionInputValue {
            label: input.label.unwrap_or_else(|| input.key.clone()),
            input_type: input.input_type.unwrap_or_else(|| "text".to_string()),
            key: input.key,
            value: input.value,
            display: input.display,
        })
        .collect();
    crate::action_run::start_action_run(
        crate::action_run::StartActionArgs {
            action_id,
            team_id,
            repo: crate::action_run::ActionRepo::Provided(repo_group),
            options,
            origin: relay_origin(cx, start.started_by.clone(), start.started_reason.clone()),
            inputs,
            target: None,
            activate_app: true,
            reservation: Some(reservation),
            // A relay start is a PERSON's start — the automation host is the
            // only caller that fires these (EXP-530).
            trigger: None,
            automation_id: None,
            on_failed: None,
        },
        cx,
    );
}

/// The §08 relay [`LaunchOrigin`] for the signed-in account: the persistent
/// device id + the active account id (the session's audit surface, §7.1 — not
/// a branch key). `started_by` is the frame's EXP-432 requester attribution —
/// only `server`-kind devices can be shared, so on a desktop it stays `None`
/// today; the plumbing is parity with the daemon (and EXP-444's foreign-host
/// login suppression consumes it wherever it does arrive). `started_reason`
/// is EXP-679's: `agent` when another coding session asked for this start,
/// which makes the run unattended (it gets the close-out tool that ends it).
fn relay_origin(
    cx: &App,
    started_by: Option<String>,
    started_reason: Option<String>,
) -> LaunchOrigin {
    let device_id = steer::persistent_device_id(&AuthContext::global(cx).data_dir);
    let claimant = queries::active_account(cx)
        .map(|account| account.id)
        .unwrap_or_default();
    LaunchOrigin::Relay {
        device_id,
        claimant,
        started_by,
        started_reason,
    }
}

/// The first shell window (one with a terminal dock). A relay start can't
/// host a coding tab on a non-shell window (login), so `None` means no
/// window is open to run in — the caller logs and drops the start.
pub(crate) fn find_team_window(cx: &mut App) -> Option<gpui::AnyWindowHandle> {
    cx.windows().into_iter().find(|handle| {
        handle
            .update(cx, |_, window, cx| {
                coding_flow::window_terminal_manager(window, cx).is_some()
            })
            .unwrap_or(false)
    })
}

/// Relay single-issue start (§08) — the button's `build_launch` sequence with
/// `LaunchOrigin::Relay`.
fn remote_issue_start(issue_id: String, start: &steer::RemoteStart, cx: &mut App) {
    // Dedup: never launch a second session for an issue this process is
    // already coding. Without this, a relay `start_session` arriving while a
    // session is live (a phone tapping "Start on my desktop" for an issue
    // already coding locally, or two taps in a row) would spawn a second
    // `claude` into the SAME `exp/<ID>` worktree and orphan the first — the
    // first child keeps running, its row never ends on tab-close, and Stop
    // only reaches the second. The button path is already guarded by its
    // Coding…/Stop render state; this closes the relay entry (LocalSessions is
    // process-global, so this covers every window).
    if coding_flow::LocalSessions::global(cx)
        .read(cx)
        .get(&issue_id)
        .is_some()
    {
        log::info!("steer: remote start for {issue_id} ignored — already coding this issue");
        return;
    }
    // REV2-24: the cross-device half of the same rule, mirroring the dialog's
    // blocker. The remote client hides Start when it SEES a live session, but
    // that view lags the launch (the row only exists once a desktop spins up
    // and round-trips through Electric), so two devices dispatched inside that
    // window would both pass the LocalSessions check and push to the same
    // `exp/<ID>` branch.
    let now = chrono::Utc::now().timestamp();
    if let Some(device) = queries::live_session_device_for_issue(cx, &issue_id, now) {
        log::info!(
            "steer: remote start for {issue_id} ignored — live session on {device} (one session per issue)"
        );
        return;
    }

    let origin = relay_origin(cx, start.started_by.clone(), start.started_reason.clone());
    // The remote client's Start-coding dialog choices (EXP-149), settings
    // defaults for anything it didn't send. Plan mode stays OFF unless the
    // client explicitly opted in (F7: an option-less start must never park
    // at a native plan-approval TUI menu on an unattended desktop — nobody
    // is at the keyboard to approve it).
    let settings = coding_flow::CodingHub::global(cx).read(cx).settings.clone();
    let options = LaunchOptions::remote(
        &settings,
        start.agent.as_deref(),
        start.model.as_deref(),
        start.effort.as_deref(),
        start.ultracode,
        start.plan_mode,
    );
    // EXP-481/EXP-662: honor the remote resume flag against the RUN REGISTRY
    // — the newest resumable record for this issue on this account relaunches
    // that exact transcript; with no record the flag degrades to a fresh
    // session seeded with the resume prompt, so an optimistic flag is always
    // safe.
    let data_dir = coding_flow::coding_data_dir(cx);
    let resume_record = start
        .resume
        .then(|| queries::active_account(cx))
        .flatten()
        .and_then(|account| {
            coding::run_registry::latest_for_issue(&data_dir, &account.id, &issue_id)
        });
    let Some((prepare_request, deps)) = (match resume_record {
        Some(record) => coding_flow::build_resume_deps(&record, cx).map(|deps| {
            (
                PrepareRequest::ResumeRun(ResumeRunRequest {
                    record,
                    device_label: coding::default_device_label(),
                    origin,
                    // The recorded run keeps its own agent and options
                    // (D2) — a remote resume nudges neither.
                    model: None,
                    effort: None,
                }),
                deps,
            )
        }),
        None => coding_flow::build_launch(&issue_id, origin, options, start.resume, cx)
            .map(|(request, deps)| (PrepareRequest::Issue(request), deps)),
    }) else {
        log::warn!("steer: remote start for {issue_id} ignored — not signed in / not synced");
        return;
    };

    let Some(target) = find_team_window(cx) else {
        log::warn!("steer: remote start for {issue_id} — no shell window open");
        return;
    };

    let hooks = hook_setup(cx);
    let observer = observer_setup(cx);
    cx.spawn(async move |cx| {
        let prepared = cx
            .background_executor()
            .spawn(async move {
                prepare_with_hooks(&prepare_request, &deps, hooks.as_ref(), observer.as_ref())
            })
            .await;
        let _ = target.update(cx, |_, window, cx| match prepared {
            Ok(Prepared::Ready(prepared)) => {
                if let Err(message) = coding_flow::spawn_into_window(
                    prepared,
                    coding_flow::SessionSubject::Issue(issue_id),
                    window,
                    cx,
                ) {
                    log::warn!("steer: remote start spawn failed: {message}");
                }
            }
            Ok(Prepared::Disabled(reason)) => {
                log::warn!("steer: remote start disabled — {}", reason.message());
            }
            Err(err) => log::warn!("steer: remote start prepare failed: {err}"),
        });
    })
    .detach();
}

/// Relay BATCH start (§08 / EXP-106) — ONE session over `issue_ids` on a
/// fresh `exp/batch-<id8>` branch. The batch equivalent of the dialog's
/// `batch_request` + `run_prepare` tail: resolve every issue from the local
/// sync store (the desktop syncs no repositories collection, so the repo rides
/// the frame), then `PrepareRequest::Batch` → `spawn_into_window`.
fn remote_batch_start(
    issue_ids: Vec<String>,
    team_id: String,
    repo: steer::StartRepoGroup,
    start: &steer::RemoteStart,
    cx: &mut App,
) {
    // No worktree dedup (unlike the issue branch): each batch run mints a
    // fresh `exp/batch-<id8>` branch, so there is never a collision to guard
    // against — but the EXP-202 one-session-per-issue rule still applies per
    // MEMBER issue (REV2-24), otherwise a batch duplicates work an agent is
    // already doing on `exp/<ID>` elsewhere and lands it in a second PR.

    // Resolve the checked issues from sync. Unknown ids are skipped; a
    // resolved issue whose board is outside the claimed team aborts the
    // WHOLE batch — a remote client must never steer this desktop into coding
    // issues from another team than the one it claimed. An issue already
    // being coded (this process or, per the synced rows, any device) aborts
    // the whole batch too, mirroring the dialog's blocker: silently dropping
    // it would run a batch the requester never asked for.
    let now = chrono::Utc::now().timestamp();
    // EXP-712: the board the batch branch is cut from — the first resolved
    // issue's. `steer.startSession` already refused a batch whose boards
    // disagree on the base branch, so any of them names the same branch.
    let mut batch_board: Option<String> = None;
    let issues: Vec<BatchIssueSpec> = {
        let store = Store::global(cx);
        let issues_coll = store.collections().issues.read(cx);
        let boards_coll = store.collections().boards.read(cx);
        let local = coding_flow::LocalSessions::global_ref(cx);
        let mut specs = Vec::new();
        for issue_id in &issue_ids {
            let Some(issue) = issues_coll.get(issue_id) else {
                log::warn!("steer: remote batch start — unknown issue {issue_id}, skipped");
                continue;
            };
            let issue_ws = boards_coll
                .get(&issue.board_id)
                .map(|board| board.team_id.as_str());
            if issue_ws != Some(team_id.as_str()) {
                log::warn!(
                    "steer: remote batch start aborted — issue {} is not in team {team_id}",
                    issue.identifier
                );
                return;
            }
            if local
                .as_ref()
                .is_some_and(|sessions| sessions.read(cx).get(issue_id).is_some())
            {
                log::warn!(
                    "steer: remote batch start aborted — already coding {} on this device",
                    issue.identifier
                );
                return;
            }
            if let Some(device) = queries::live_session_device_for_issue(cx, issue_id, now) {
                log::warn!(
                    "steer: remote batch start aborted — {} has a live session on {device}",
                    issue.identifier
                );
                return;
            }
            batch_board.get_or_insert_with(|| issue.board_id.clone());
            specs.push(BatchIssueSpec {
                issue_id: issue.id.clone(),
                issue_identifier: issue.identifier.clone(),
                title: issue.title.clone(),
                description: issue.description.clone(),
                status: issue.status,
            });
        }
        specs
    };
    if issues.is_empty() {
        log::warn!("steer: remote batch start aborted — no issues resolved from sync");
        return;
    }

    // Absent options fall to the settings defaults (EXP-206: same set as an
    // issue start); plan mode stays OFF unless the remote client opted in
    // (F7 — same unattended-desktop rule as the issue branch).
    let settings = coding_flow::CodingHub::global(cx).read(cx).settings.clone();
    let options = LaunchOptions::remote(
        &settings,
        start.agent.as_deref(),
        start.model.as_deref(),
        start.effort.as_deref(),
        start.ultracode,
        start.plan_mode,
    );

    // Same field construction the dialog's `batch_request` uses (device_label
    // from `coding::default_device_label()`, a fresh `coding::new_batch_id()`).
    let batch_id = coding::new_batch_id();
    let request = BatchLaunchRequest {
        batch_id: batch_id.clone(),
        team_id,
        board_id: batch_board,
        repo: RepoGroup {
            repository_id: repo.repository_id,
            full_name: repo.full_name,
            default_branch: repo.default_branch,
        },
        issues,
        device_label: coding::default_device_label(),
        origin: relay_origin(cx, start.started_by.clone(), start.started_reason.clone()),
        options,
    };

    let Some(deps) = coding_flow::build_batch_deps(cx) else {
        log::warn!("steer: remote batch start ignored — not signed in / not synced");
        return;
    };
    let Some(target) = find_team_window(cx) else {
        log::warn!("steer: remote batch start — no shell window open");
        return;
    };

    let hooks = hook_setup(cx);
    let observer = observer_setup(cx);
    cx.spawn(async move |cx| {
        let prepared = cx
            .background_executor()
            .spawn(async move {
                prepare_with_hooks(
                    &PrepareRequest::Batch(request),
                    &deps,
                    hooks.as_ref(),
                    observer.as_ref(),
                )
            })
            .await;
        let _ = target.update(cx, |_, window, cx| match prepared {
            Ok(Prepared::Ready(prepared)) => {
                if let Err(message) = coding_flow::spawn_into_window(
                    prepared,
                    coding_flow::SessionSubject::Batch(batch_id),
                    window,
                    cx,
                ) {
                    log::warn!("steer: remote batch start spawn failed: {message}");
                }
            }
            Ok(Prepared::Disabled(reason)) => {
                log::warn!("steer: remote batch start disabled — {}", reason.message());
            }
            Err(err) => log::warn!("steer: remote batch start prepare failed: {err}"),
        });
    })
    .detach();
}

// ---------------------------------------------------------------------------
// Publisher registry (§8.4/§8.5) — the sessions this process is publishing
// ---------------------------------------------------------------------------

struct PublisherEntry {
    handle: PublisherHandle,
    /// §P7: the activity emitter's run flag (members-only activity channel);
    /// flipping it `false` stops the thread on teardown.
    activity_active: Arc<AtomicBool>,
    /// EXP-637: the emitter's turn-state signal — the graceful stop waits on
    /// it so an agent that just called `exponential_sessions_end` finishes
    /// writing its close-out before anything tears the child down.
    turn_signal: Arc<steer::TurnSignal>,
}

/// Session-keyed publisher handles — parallels `coding_flow::LocalSessions`
/// but holds the steer side (the publisher task handle).
#[derive(Default)]
pub struct PublisherRegistry {
    entries: HashMap<String, PublisherEntry>,
}
struct PublisherRegistryGlobal(Entity<PublisherRegistry>);
impl Global for PublisherRegistryGlobal {}

impl PublisherRegistry {
    fn global(cx: &mut App) -> Entity<PublisherRegistry> {
        if let Some(g) = cx.try_global::<PublisherRegistryGlobal>() {
            return g.0.clone();
        }
        let entity = cx.new(|_| PublisherRegistry::default());
        cx.set_global(PublisherRegistryGlobal(entity.clone()));
        entity
    }

    fn global_ref(cx: &App) -> Option<Entity<PublisherRegistry>> {
        cx.try_global::<PublisherRegistryGlobal>()
            .map(|g| g.0.clone())
    }
}

/// Marshaled from the publisher task (steer runtime) to the gpui foreground.
enum SteerUiEvent {
    /// A surfaced publisher error (clock skew, repeated rejects — §8.7).
    Error(String),
    /// End the session: a relay `kill` frame. Kills the child + stops the
    /// publisher; drains stop after.
    Teardown,
    /// EXP-637: the own-row Electric kill (§8.8) with the ended row's
    /// close-out. Which teardown it gets is [`ended_policy`]'s call — an
    /// AGENT-declared end is the run finishing itself, not a kill.
    Ended(sync::kill_watch::EndedFacts),
}

/// The per-session facts the emitter needs off a `PreparedLaunch`, snapshotted
/// by `coding_flow::spawn_into_window` before the spawn consumes it.
pub struct SteerSessionInfo {
    /// EXP-275: the launch's resolved permission posture (bypass on: a
    /// non-pi agent outside plan mode) — feeds the emitter's launch narration. Bypass
    /// sessions still hit real permission prompts (claude flags dangerous
    /// commands even then, EXP-564), so nothing else keys on it.
    pub bypass_permissions: bool,
    /// EXP-529: the launch started in plan mode — stamped into the emitter's
    /// launch narration so remote viewers can tell the run's posture.
    pub plan_mode: bool,
    /// EXP-383: which agent CLI the session runs — selects the activity
    /// emitter and gates the claude-only hook/answer machinery off for
    /// codex/pi.
    pub agent: CodingAgent,
    /// EXP-443: the launcher-minted `--session-id` (fresh claude sessions).
    pub claude_session_id: Option<String>,
    /// EXP-443: the launcher-stamped codex rollout originator.
    pub codex_originator: Option<String>,
    /// EXP-443: the exact rollout id a codex native resume reopens.
    pub codex_resume_id: Option<String>,
    /// EXP-432: the requesting teammate on a shared-device relay start
    /// (`heartbeat_scope.started_by_id`) — `None` on local/own starts.
    pub started_by_id: Option<String>,
    /// EXP-688: the ref the published diff is measured from
    /// (`origin/<default branch>`) — see [`steer::activity::worktree_diff`].
    pub base_ref: Option<String>,
}

/// Attach a steer publisher to a freshly launched coding session (§8.4). The
/// single call `coding_flow::spawn_into_window` makes on `LaunchOutcome::
/// Spawned` — for BOTH subjects (issue sessions and multi-issue batch
/// runs; a batch session publishes with `issue_id: None` and is
/// never publicly fanned). Best-effort and non-blocking: a disabled/
/// unreachable relay ends the publisher task quietly and the session keeps
/// running locally.
pub fn attach_publisher(
    session_id: &str,
    subject: &coding_flow::SessionSubject,
    tab: TabId,
    manager: &Entity<TerminalManager>,
    worktree: PathBuf,
    info: SteerSessionInfo,
    cx: &mut App,
) {
    let SteerSessionInfo {
        bypass_permissions,
        plan_mode,
        agent,
        claude_session_id,
        codex_originator,
        codex_resume_id,
        started_by_id,
        base_ref,
    } = info;
    let session_agent = match agent {
        CodingAgent::Claude => steer::activity::SessionAgent::Claude,
        CodingAgent::Codex => steer::activity::SessionAgent::Codex,
        CodingAgent::Pi => steer::activity::SessionAgent::Pi,
    };
    let is_claude = session_agent == steer::activity::SessionAgent::Claude;
    // EXP-455: keystroke-choreographed remote answering — claude's pickers
    // and codex's approval modals. Pi steers through its observer extension,
    // but its ONE answerable question — the plan-approval confirm dialog
    // (EXP-441) — resolves by keystroke too, so the answer seam covers it.
    let steers_by_keystroke =
        is_claude || session_agent == steer::activity::SessionAgent::Codex;
    let answers_remotely =
        steers_by_keystroke || session_agent == steer::activity::SessionAgent::Pi;
    let Some(runtime) = runtime(cx) else {
        return; // steer off (runtime failed to init)
    };
    let Some(trpc) = queries::trpc_client(cx) else {
        return; // signed out — nothing to publish as
    };
    let trpc = Arc::new(trpc);

    // Foreground: read the Send+Sync PTY handles off the live terminal (the
    // §6.14 seam — the publisher never opens its own PTY or re-reads).
    let Some(view) = manager.read(cx).tab(tab).map(|tab| tab.view.clone()) else {
        return;
    };
    let (writer, term) = {
        let session = view.read(cx).session().borrow();
        (session.writer(), session.term())
    };

    // The foreground-marshal channel for the non-`Send`-handle hooks.
    let (ui_tx, ui_rx) = flume::unbounded::<SteerUiEvent>();
    let error_tx = ui_tx.clone();
    let kill_tx = ui_tx.clone();

    // Remote input → the ONE shared PTY writer (Send+Sync Arc, no gpui); the
    // TermHandle is the EXP-72 bracketed-paste gate for text frames. The
    // EXP-249 answer choreography injects through the SAME hook, so the child
    // cannot tell a steerer's answer from local typing.
    let write_input = pty_writer_input_hook(writer, term.clone());
    // EXP-249: `answer` frames land on the publisher and are executed by the
    // emitter, which owns question identity and the live grid.
    let (answer_link, answers) = AnswerLink::new();

    // EXP-383: a pi session's observer subscription — the emitter drains the
    // event stream, the publisher pushes remote composer messages into the
    // steer queue the extension long-polls (applied via pi.sendUserMessage).
    let pi_observer = (session_agent == steer::activity::SessionAgent::Pi)
        .then(|| {
            cx.try_global::<PiObserverGlobal>()
                .map(|global| global.0.subscribe(&worktree))
        })
        .flatten();
    let (pi_events, pi_steer) = match pi_observer {
        Some((events, steer_handle)) => (Some(events), Some(steer_handle)),
        None => (None, None),
    };
    // EXP-724: the remote slash-command seam. The publisher RECOGNISES a
    // catalog command and hands it whole to the emitter, which owns the grid
    // and the turn state typing one needs. Pi is the exception that proves
    // it: its commands never touch the PTY at all, so its sink pushes
    // `{name, args}` onto the same observer-extension queue the text sink
    // uses, and the extension calls pi's own APIs.
    let command_link = CommandLink::new(pi_steer.clone().map(|handle| {
        Arc::new(move |name: &str, args: &str| handle.push_command(name, args))
            as steer::CommandSink
    }));

    let hooks = PublisherHooks {
        write_input: write_input.clone(),
        // The rest marshal to the foreground (they touch the gpui-held term).
        kill: Arc::new(move |_signal| {
            let _ = kill_tx.send(SteerUiEvent::Teardown);
        }),
        error: Arc::new(move |message| {
            let _ = error_tx.send(SteerUiEvent::Error(message));
        }),
        // EXP-383/EXP-455: the semantic answer path drives the claude and
        // codex TUIs by grid keystroke choreography (claude: plan/ask/login/
        // permission pickers; codex: approval modals). Pi carries it ONLY
        // for the plan gate (EXP-441) — its free text short-circuits into
        // `text_sink` first, so the publisher's Enter-cascade/Esc-reroute
        // logic stays inert (the pi emitter never sets the link flags).
        answers: answers_remotely.then(|| answer_link.clone()),
        agent: session_agent,
        text_sink: pi_steer.map(|handle| {
            Arc::new(move |text: String| handle.push(text)) as Arc<dyn Fn(String) + Send + Sync>
        }),
        // EXP-511: a steered message's image embeds are downloaded with this
        // account's own auth into the worktree and handed to the agent as
        // plain file paths.
        attachments: Some(steer::image_localizer(
            trpc.clone(),
            worktree.join(coding::launcher::STEER_IMAGES_DIR),
        )),
        commands: Some(command_link.clone()),
    };

    // EXP-214: the needs-input forwarder's own handle — cloned before the
    // tickets take ownership of `trpc`.
    let needs_input_trpc = trpc.clone();
    let tickets: Arc<dyn PublisherTickets> = Arc::new(TrpcPublisherTickets {
        trpc,
        coding_session_id: session_id.to_string(),
    });
    let spec = PublishSpec {
        session_id: session_id.to_string(),
        // Batch sessions publish an issue-less room (the field is already
        // Option): no issue page ever surfaces them, viewers reach them by
        // session id only.
        issue_id: match subject {
            coding_flow::SessionSubject::Issue(issue_id) => Some(issue_id.clone()),
            coding_flow::SessionSubject::Batch(_)
            | coding_flow::SessionSubject::Action(_) => None,
        },
    };
    let handle = steer::publish(&runtime, spec, tickets, hooks);

    // §P7: start the activity emitter — the desktop emits scrubbed activity
    // events over the publisher socket for authenticated team members on
    // the relay's activity channel (the anonymous public audience was removed
    // in EXP-90). The emitter tails the Claude transcript + worktree diffs
    // and redacts before sending. Best-effort: a relay-disabled instance just
    // drops the sends.
    let activity_active = Arc::new(AtomicBool::new(true));
    // EXP-637: the emitter flips it on every turn boundary; the graceful
    // stop waits on it before tearing an agent-ended run down.
    let turn_signal = Arc::new(steer::TurnSignal::new());
    // EXP-214: forward the emitter's picker-pending flips to the synced
    // `coding_sessions.needs_input` column so every client can badge
    // "Needs input". Runs on the emitter thread (blocking HTTP is fine
    // there); the return value tells the emitter whether the write landed so
    // a failed one retries instead of sticking the badge (EXP-355) — a
    // swept/ended row reports `updated: false`, which still counts as landed.
    let needs_input_session = session_id.to_string();
    // REV2-17: the `expu_` personal key from the account's secret store —
    // codex/pi sessions carry it only in the spawn env (never a worktree
    // file), so the redactor's exact-match layer needs it handed in here.
    // The store always holds the current key: the launcher's
    // `ensure_personal_key` reads-or-mints it there before any spawn.
    let extra_secrets: Vec<String> = cx
        .try_global::<AuthContext>()
        .map(|auth| auth.data_dir.clone())
        .zip(queries::active_account(cx))
        .and_then(|(data_dir, account)| {
            api::token_store::TokenStore::new(data_dir).get(
                &account.id,
                api::token_store::SecretKind::PersonalApiKey,
            )
        })
        .into_iter()
        .collect();
    // EXP-249: this session's slice of the hooks sidecar — the structured
    // plan/question/subagent/permission stream. Absent when the sidecar never
    // came up; the emitter then runs grid-only, exactly as before. Claude
    // only (EXP-383): the sidecar is fed by claude's `--settings` hooks, so
    // a codex/pi subscription would just leak.
    let hook_events = is_claude
        .then(|| {
            cx.try_global::<HookSidecarGlobal>()
                .map(|global| global.0.subscribe(&worktree, claude_session_id.as_deref()))
        })
        .flatten();
    // EXP-444/EXP-432: a relay start whose requester is NOT this signed-in
    // account runs on a shared host — the emitter suppresses the remote
    // login flow for it.
    let foreign_host = started_by_id.as_deref().is_some_and(|requester| {
        queries::active_account(cx)
            .map(|account| account.user_id)
            .as_deref()
            != Some(requester)
    });
    spawn_activity_emitter(
        EmitterConfig {
            agent: session_agent,
            worktree,
            base_ref,
            extra_secrets,
            // The live grid: the emitter watches it to confirm pickers the
            // transcript can't show while PENDING (EXP-150) and to choreograph
            // a remote answer's keystrokes (EXP-249).
            term: Some(term),
            on_needs_input: Some(Arc::new(move |pending| {
                match api::coding_sessions::set_needs_input(
                    &needs_input_trpc,
                    &needs_input_session,
                    pending,
                ) {
                    Ok(_) => true,
                    Err(err) => {
                        log::debug!("steer: setNeedsInput({pending}) failed: {err}");
                        false
                    }
                }
            })),
            hooks: hook_events,
            // EXP-383/EXP-455: the Steering seam drives the claude TUI's
            // pickers and codex's approval modals by keystroke; pi rides it
            // for the plan-approval confirm dialog only (EXP-441).
            steering: answers_remotely.then(|| Steering {
                answers,
                link: answer_link,
                write_input,
                commands: Some(command_link),
            }),
            bypass_permissions,
            plan_mode,
            pi_events,
            claude_session_id,
            codex_originator,
            codex_resume_id,
            foreign_host,
            turn_signal: Some(turn_signal.clone()),
        },
        handle.activity_sender(),
        activity_active.clone(),
    );

    // Register the session (teardown bookkeeping).
    let registry = PublisherRegistry::global(cx);
    registry.update(cx, |registry, _| {
        registry.entries.insert(
            session_id.to_string(),
            PublisherEntry {
                handle,
                activity_active,
                turn_signal,
            },
        );
    });

    // §8.8 own-row kill-switch: end the session when the synced row flips to
    // `ended` even if the relay is unreachable. The callback is cx-free, so it
    // routes the teardown through the same foreground drain. The signed-in
    // user's id pins the row's expected owner (EXP-105 F3): a swept-then-
    // resurrected row carries the resurrector as owner, and its `ended` flip
    // must never kill this run.
    if let Some(kill_watch) = cx.try_global::<KillWatchGlobal>().map(|g| g.0.clone()) {
        let own_user_id = queries::active_account(cx).map(|account| account.user_id);
        let teardown_tx = ui_tx.clone();
        kill_watch.update(cx, |watch, cx| {
            watch.watch(
                session_id.to_string(),
                own_user_id,
                Box::new(move |facts| {
                    let _ = teardown_tx.send(SteerUiEvent::Ended(facts));
                }),
                cx,
            );
        });
    }

    // The per-session foreground drain: apply marshaled events with `cx`.
    let session_id = session_id.to_string();
    let manager_weak = manager.downgrade();
    cx.spawn(async move |cx| {
        while let Ok(event) = ui_rx.recv_async().await {
            let torn_down =
                cx.update(|cx| apply_steer_event(&session_id, &manager_weak, tab, event, cx));
            if torn_down {
                break;
            }
        }
    })
    .detach();
}

/// Apply one marshaled steer event on the gpui foreground. Returns `true` when
/// it tore the session down (the drain then stops).
fn apply_steer_event(
    session_id: &str,
    manager: &WeakEntity<TerminalManager>,
    tab: TabId,
    event: SteerUiEvent,
    cx: &mut App,
) -> bool {
    match event {
        SteerUiEvent::Error(message) => {
            log::warn!("steer publisher [{session_id}]: {message}");
            false
        }
        SteerUiEvent::Teardown => {
            teardown_session(session_id, manager, tab, cx);
            true
        }
        SteerUiEvent::Ended(facts) => apply_ended(session_id, manager, tab, facts, cx),
    }
}

/// EXP-637/EXP-673 — what to do when a watched session's row flips to
/// `ended`. Every ended row closes its tab: an open tab under a run the
/// server already ended is a leftover (EXP-673), and there are many of them
/// once automations run unattended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EndPolicy {
    /// Tear the tab down at once — a kill, a client end, a merge, the sweep.
    CloseNow,
    /// The agent declared its own run over (`exponential_sessions_end`): let
    /// the turn finish (it is still writing the close-out that call was
    /// about), then close the tab. Since EXP-673 the server ends a row on
    /// that call ONLY for an automation-started run — a person-started run
    /// stays live for their replies and never reaches here — so this is
    /// always a tab nobody is watching.
    CloseAfterTurn,
}

/// The policy for one ended row.
pub(crate) fn ended_policy(ended_by: Option<&str>) -> EndPolicy {
    if ended_by == Some(domain::contract::CODING_SESSION_ENDED_BY_AGENT) {
        EndPolicy::CloseAfterTurn
    } else {
        EndPolicy::CloseNow
    }
}

/// Apply an ended row's [`EndPolicy`]. Returns `true` when the per-session
/// drain should stop (every policy ends the session one way or another).
fn apply_ended(
    session_id: &str,
    manager: &WeakEntity<TerminalManager>,
    tab: TabId,
    facts: sync::kill_watch::EndedFacts,
    cx: &mut App,
) -> bool {
    let policy = ended_policy(facts.ended_by.as_deref());
    log::info!("steer [{session_id}]: row ended ({:?}) → {policy:?}", facts.ended_by);
    let signal = PublisherRegistry::global_ref(cx).and_then(|registry| {
        registry
            .read(cx)
            .entries
            .get(session_id)
            .map(|entry| entry.turn_signal.clone())
    });
    match policy {
        EndPolicy::CloseNow => {
            teardown_session(session_id, manager, tab, cx);
        }
        EndPolicy::CloseAfterTurn => {
            let session_id = session_id.to_string();
            let manager = manager.clone();
            crate::graceful_stop::after_turn(
                &session_id.clone(),
                signal,
                move |cx| teardown_session(&session_id, &manager, tab, cx),
                cx,
            );
        }
    }
    true
}

/// End a published session (relay `kill` or own-row Electric kill, §8.4/§8.8):
/// close the WHOLE terminal tab (EXP-268 — a killed session must not leave a
/// dead tab with an exit strip behind; `close_tab` kills the child and joins
/// the PTY threads), stop the publisher, drop the kill-watch, and forget the
/// session. The tab-close path also fires the `TabClosed` watcher, which
/// handles the `codingSessions.end` bookkeeping and closes any undocked window
/// hosting the tab.
fn teardown_session(
    session_id: &str,
    manager: &WeakEntity<TerminalManager>,
    tab: TabId,
    cx: &mut App,
) {
    let Some(registry) = PublisherRegistry::global_ref(cx) else {
        return;
    };
    if !registry.read(cx).entries.contains_key(session_id) {
        return; // already torn down
    }

    // Close the tab — kill + join ride `close_tab`'s session shutdown. The
    // relay never reaches this on the dead-relay path; this is the durable
    // abort.
    if let Some(manager) = manager.upgrade() {
        manager.update(cx, |manager, cx| manager.close_tab(tab, cx));
    }

    detach_publisher(session_id, Some("killed".to_string()), cx);
}

/// Detach the steer side of a session WITHOUT touching its terminal tab
/// (EXP-283): stop the publisher with a clean `bye {outcome}`, stop the §P7
/// activity emitter, forget the registry entry, and drop the kill-watch
/// registration. Idempotent — a no-op for sessions this process is not
/// (or no longer) publishing.
///
/// This is the child-exit edge's steer cleanup: the exit hook's own
/// `codingSessions.end` flips the synced row to `ended`, and without the
/// unwatch here the §8.8 kill-watch would read our OWN end back from Electric
/// as a remote kill and close the exited tab — which must stay open with the
/// exit strip (§7.5 keep-tab-open semantics).
pub fn detach_publisher(session_id: &str, outcome: Option<String>, cx: &mut App) {
    if let Some(registry) = PublisherRegistry::global_ref(cx) {
        registry.update(cx, |registry, cx| {
            if let Some(entry) = registry.entries.remove(session_id) {
                // Stop the publisher (idempotent).
                entry.handle.shutdown(outcome);
                // §P7: stop the activity emitter thread promptly.
                entry.activity_active.store(false, Ordering::SeqCst);
                cx.notify();
            }
        });
    }

    // Drop the kill-watch registration so a later row change can't re-fire.
    if let Some(kill_watch) = cx.try_global::<KillWatchGlobal>().map(|g| g.0.clone()) {
        kill_watch.update(cx, |watch, _| watch.unwatch(session_id));
    }
}


#[cfg(test)]
mod tests {
    /// EXP-673: every ended row closes its tab. The agent's own close-out
    /// waits out the turn (it is still writing); every other end is now.
    #[test]
    fn every_ended_row_closes_the_tab() {
        use super::{ended_policy, EndPolicy};
        assert_eq!(ended_policy(Some("agent")), EndPolicy::CloseAfterTurn);
        for ended_by in [Some("user"), Some("client"), Some("merge"), Some("system"), None] {
            assert_eq!(ended_policy(ended_by), EndPolicy::CloseNow, "{ended_by:?}");
        }
    }

    use super::*;

    fn subscriber(
        subscribers: &Arc<Mutex<Vec<HookSubscriber>>>,
        worktree: &Path,
        session_id: Option<&str>,
    ) -> flume::Receiver<HookEvent> {
        let (tx, rx) = flume::unbounded();
        let mut guard = match subscribers.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        guard.push(HookSubscriber {
            // Like the real `subscribe`: on macOS `temp_dir()` sits behind
            // the `/var` → `/private/var` symlink, and the router compares
            // CANONICAL paths — a raw path here fails every cwd match.
            worktree: canonical(worktree),
            tx,
            bound: session_id
                .map(|id| HashSet::from([id.to_string()]))
                .unwrap_or_default(),
        });
        rx
    }

    fn event(session_id: Option<&str>, cwd: &Path) -> HookEvent {
        steer::hooks::parse_hook_event(
            serde_json::json!({
                "hook_event_name": "Stop",
                "session_id": session_id,
                "cwd": cwd.to_string_lossy(),
            })
            .to_string()
            .as_bytes(),
        )
        .expect("fixture parses")
    }

    /// EXP-443 (twin of `cli::sidecars`' router tests — the two bodies must
    /// stay identical): a pre-seeded bound id wins over an earlier same-cwd
    /// subscriber.
    #[test]
    fn router_prefers_the_bound_session_id() {
        let subscribers: Arc<Mutex<Vec<HookSubscriber>>> = Arc::default();
        let cwd = std::env::temp_dir();
        let first = subscriber(&subscribers, &cwd, Some("sess-a"));
        let second = subscriber(&subscribers, &cwd, Some("sess-b"));
        route_hook_event(&subscribers, event(Some("sess-b"), &cwd));
        assert!(second.try_recv().is_ok(), "bound id must win");
        assert!(first.try_recv().is_err());
    }

    #[test]
    fn router_gives_an_unknown_id_to_the_unbound_subscriber() {
        let subscribers: Arc<Mutex<Vec<HookSubscriber>>> = Arc::default();
        let cwd = std::env::temp_dir();
        let seeded = subscriber(&subscribers, &cwd, Some("sess-a"));
        let unseeded = subscriber(&subscribers, &cwd, None);
        route_hook_event(&subscribers, event(Some("sess-c"), &cwd));
        assert!(unseeded.try_recv().is_ok());
        assert!(seeded.try_recv().is_err());
    }

    /// EXP-505: a duplicate remote action frame must fail the claim while
    /// the first start is in flight; dropping the guard (the pipeline
    /// finishing, on ANY path) frees the key for the next start.
    #[test]
    fn action_reservation_is_all_or_nothing_until_released() {
        let reservations = StartReservations::default();
        let first = reservations.claim(String::from("action:a"));
        assert!(first.is_some());
        assert!(
            reservations.claim(String::from("action:a")).is_none(),
            "duplicate frame must be dropped while the first is in flight"
        );
        assert!(
            reservations.claim(String::from("action:b")).is_some(),
            "an unrelated action must not contend"
        );
        drop(first);
        assert!(
            reservations.claim(String::from("action:a")).is_some(),
            "a released claim must free the key"
        );
    }

    #[test]
    fn router_binds_a_rotated_id_by_cwd_as_last_resort() {
        let subscribers: Arc<Mutex<Vec<HookSubscriber>>> = Arc::default();
        let cwd = std::env::temp_dir();
        let seeded = subscriber(&subscribers, &cwd, Some("sess-a"));
        route_hook_event(&subscribers, event(Some("sess-rotated"), &cwd));
        assert!(seeded.try_recv().is_ok(), "cwd fallback must deliver");
        let unseeded = subscriber(&subscribers, &cwd, None);
        route_hook_event(&subscribers, event(Some("sess-rotated"), &cwd));
        assert!(seeded.try_recv().is_ok());
        assert!(unseeded.try_recv().is_err());
    }
}
