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
//! 2. **Publisher — the ENGINE's (EXP-746 D14, EXP-773).** A coding session
//!    runs in the in-process ACP engine, which owns its publisher, its
//!    activity vocabulary and its `bye`. Nothing here attaches one.
//!
//! 3. **Kill-switch — the own-row Electric watch (§8.8).** Every session is
//!    registered with the [`sync::KillWatch`]; when its `coding_sessions` row
//!    flips to `ended` (a `steer.killSession` DB write that reaches us over
//!    sync even when the relay is dead), [`register_kill_feed`] turns it into
//!    an [`engine::KillFeed`] edge — the only kill path that survives a dead
//!    relay.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use gpui::{App, AppContext as _, Entity, Global};

use coding::{
    prepare, BatchIssueSpec, BatchLaunchRequest, LaunchOptions, LaunchOrigin, Prepared,
    PrepareRequest, RepoGroup, ResumeRunRequest,
};
use steer::{
    spawn_control_channel, ControlApi, ControlChannelHandle, DeviceIdentity, PublisherTickets,
    SteerRuntime, TrpcControlApi, TrpcPublisherTickets,
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
    let _ = ControlChannels::global(cx);

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
    // EXP-773: the transcript store this account's runs write to, and the
    // one-replay-per-session guard the `history_request` handler takes.
    let history_dir = auth.data_dir.clone();
    let history_trpc = Arc::clone(&trpc);
    let history_runtime = Arc::clone(&runtime);
    let history_in_flight = steer::HistoryInFlight::new();
    // EXP-796: the same store answers "Load earlier" pages of a run this
    // machine already replayed, back down the control socket.
    let page_dir = auth.data_dir.clone();
    let page_runtime = Arc::clone(&runtime);
    // Boot pass: drop journals nobody can ask for anymore (60 days).
    {
        let prune_dir = history_dir.clone();
        cx.background_executor()
            .spawn(async move {
                steer::prune_journals(&prune_dir, steer::JOURNAL_MAX_AGE);
            })
            .detach();
    }
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
            // EXP-773: serve a stored transcript back to the relay. Reading
            // and republishing both happen on the steer runtime — the socket
            // loop only hands the id over.
            let on_history_request: steer::HistoryRequestFn = Arc::new(move |session_id| {
                let tickets: Arc<dyn PublisherTickets> = Arc::new(TrpcPublisherTickets {
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
            let on_history_page: steer::HistoryPageFn = Arc::new(move |ask, reply| {
                steer::serve_history_page(&page_runtime, page_dir.clone(), ask, reply);
            });
            let control_api: Arc<dyn ControlApi> = Arc::new(TrpcControlApi(trpc));
            let handle = spawn_control_channel(
                &runtime,
                device,
                control_api,
                on_start,
                on_check_in,
                on_history_request,
                on_history_page,
            );

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

/// EXP-746 (D9): the caps this device advertises — ONE list, owned by
/// [`coding::doctor::DEVICE_CAPS`] + [`coding::doctor::ACTION_CAPS`] and
/// shared with the CLI daemon. They used to be hand-synced copies, and a
/// one-sided edit silently made one host un-targetable for the new feature.
fn device_caps(advertisement: &coding::AgentAdvertisement) -> Vec<String> {
    coding::device_caps(advertisement)
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
    // EXP-749: which of those speak ACP here. Sent even when empty — a NULL
    // column means "older build, assume all", which is a different answer.
    let acp_agents = advertisement.acp_agents.clone();
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
                    acp_agents: Some(&acp_agents),
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
        // EXP-758: and SAY so. A duplicate delivery is invisible either way,
        // but a person pressing Run twice (or a schedule firing while they
        // do) got nothing at all back from this machine, the same silence a
        // start that was never picked up produces.
        crate::action_run::notify_target_error(
            None,
            "That action is already starting on this machine.",
            cx,
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
            on_settled: None,
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

    cx.spawn(async move |cx| {
        let prepared = cx
            .background_executor()
            .spawn(async move { prepare(&prepare_request, &deps) })
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

    let prepare_request = PrepareRequest::Batch(request);
    cx.spawn(async move |cx| {
        let prepared = cx
            .background_executor()
            .spawn(async move { prepare(&prepare_request, &deps) })
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

/// EXP-746 — the account facts the engine needs off the app state.
///
/// REV2-17: the account's `expu_` personal key. It is the redactor's
/// exact-match secret (a codex/pi session carries it in the spawn env, never
/// in a worktree file) AND the bearer the engine puts on the agent's MCP
/// wiring. The store always holds the current one — the launcher's
/// `ensure_personal_key` reads-or-mints it there before any spawn.
pub(crate) fn personal_key(cx: &App) -> Option<String> {
    cx.try_global::<AuthContext>()
        .map(|auth| auth.data_dir.clone())
        .zip(queries::active_account(cx))
        .and_then(|(data_dir, account)| {
            api::token_store::TokenStore::new(data_dir).get(
                &account.id,
                api::token_store::SecretKind::PersonalApiKey,
            )
        })
}

/// EXP-444/EXP-432: a relay start whose requester is NOT the signed-in
/// account runs on a SHARED host — the login affordances stay suppressed for
/// it. `None` (a local or own start) is never foreign.
pub(crate) fn foreign_host(started_by_id: Option<&str>, cx: &App) -> bool {
    started_by_id.is_some_and(|requester| {
        queries::active_account(cx)
            .map(|account| account.user_id)
            .as_deref()
            != Some(requester)
    })
}

/// Drop `session_id`'s §8.8 own-row kill watch. Idempotent (an unwatched id
/// is a no-op), and deliberately separate from [`detach_publisher`]: an ACP
/// run has no publisher entry here at all (D14 — the engine owns its
/// publisher), but it still needs the watch dropped before its own `ended`
/// flip syncs back, or that flip reads as a remote kill (EXP-283).
pub(crate) fn unwatch_kill(session_id: &str, cx: &mut App) {
    if let Some(kill_watch) = cx.try_global::<KillWatchGlobal>().map(|g| g.0.clone()) {
        kill_watch.update(cx, |watch, _| watch.unwatch(session_id));
    }
}

/// EXP-746 — register the §8.8 kill watch for an ACP run and hand the engine
/// the receiving half.
///
/// The engine is gpui-free and runs on its own thread, so it can neither
/// register the watch (`KillWatch::watch` needs a `Context`) nor unwatch it
/// (`&mut App`). The host therefore registers HERE, with its own `cx`, BEFORE
/// `engine::start` — no edge can be missed between the row being created and
/// the first frame — and hands over a plain channel. `unwatch` marshals back
/// to the foreground through the same [`flume`] recipe every other steer
/// callback uses; the engine drops the feed FIRST in its end sequence, so the
/// run's own `ended` flip cannot bounce back at it (EXP-283).
///
/// The POLICY stays here too: [`ended_policy`] decides whether an ended row is
/// a stop-now or the agent finishing its own close-out.
pub(crate) fn register_kill_feed(session_id: &str, cx: &mut App) -> engine::KillFeed {
    let (kill_tx, kill_rx) = flume::unbounded::<engine::KillReason>();
    let (unwatch_tx, unwatch_rx) = flume::bounded::<()>(1);
    if let Some(kill_watch) = cx.try_global::<KillWatchGlobal>().map(|g| g.0.clone()) {
        let own_user_id = queries::active_account(cx).map(|account| account.user_id);
        kill_watch.update(cx, |watch, cx| {
            watch.watch(
                session_id.to_string(),
                own_user_id,
                Box::new(move |facts| {
                    let _ = kill_tx.send(kill_reason(facts.ended_by.as_deref()));
                }),
                cx,
            );
        });
    }
    let watched = session_id.to_string();
    cx.spawn(async move |cx| {
        if unwatch_rx.recv_async().await.is_err() {
            return;
        }
        let _ = cx.update(|cx| unwatch_kill(&watched, cx));
    })
    .detach();
    engine::KillFeed {
        rx: kill_rx,
        unwatch: Some(Box::new(move || {
            let _ = unwatch_tx.send(());
        })),
    }
}

/// The engine's half of [`ended_policy`]: an agent-declared end lets the turn
/// finish (`steer::STOP_GRACE` on the `TurnSignal`, waited out by the engine),
/// everything else stops now.
pub(crate) fn kill_reason(ended_by: Option<&str>) -> engine::KillReason {
    match ended_policy(ended_by) {
        EndPolicy::CloseAfterTurn => engine::KillReason::AfterTurn,
        EndPolicy::CloseNow => engine::KillReason::Now,
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;

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

}
