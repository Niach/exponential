//! App-side wiring for the remote-steer subsystem (masterplan-v3 §08) — the
//! ONE place that ties the `steer` crate, the `sync::kill_watch` own-row
//! kill-switch, and the hosted `coding` sessions together. `steer` depends on
//! neither `sync` nor `ui`, and `coding` depends on neither `steer` nor `ui`
//! (§3.1); this module is the intended meeting point ("the app/ui layer wires
//! both", `steer::lib` docs).
//!
//! The three seams, stated exactly:
//!
//! 1. **Control channel — starts with the app session.** [`install`] (from the
//!    app bootstrap, `main.rs`) creates the single [`steer::SteerRuntime`],
//!    installs the [`sync::KillWatch`] and the session pickup
//!    ([`crate::session_pickup`], EXP-1294: starts arrive as `sessions` rows,
//!    never as relay frames). [`start_control_channel`] (from
//!    `session::connect_account`, once per signed-in account) dials the
//!    device-presence socket; [`stop_control_channel`] (from
//!    `session::sign_out_active`) tears it down.
//!
//! 2. **Publisher — the ENGINE's (EXP-746 D14, EXP-773).** A coding session
//!    runs in the in-process ACP engine, which owns its publisher, its
//!    activity vocabulary and its `bye`. Nothing here attaches one.
//!
//! 3. **Kill-switch — the own-row Electric watch (§8.8).** Every hosted
//!    session is registered with the [`sync::KillWatch`]; when its `sessions`
//!    row flips to `ended` (a `sessions.end` that reaches us over sync even
//!    when the relay is dead), [`register_kill_feed`] turns it into an
//!    [`engine::KillFeed`] edge — the only kill path that survives a dead
//!    relay.

use std::collections::HashMap;
use std::sync::Arc;

use gpui::{App, AppContext as _, Entity, Global};

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
    // EXP-1249: every gpui_linux portal (file prompts, open/reveal) rides
    // zbus on tokio — the main thread holds this runtime's context for life.
    if crate::file_picker::needs_tokio_context() {
        crate::file_picker::enter_tokio_context(cx);
    }

    // §8.8 own-row Electric kill-switch: install the watch over the shared
    // `sessions` collection.
    let store = Store::global(cx).clone();
    let kill_watch = KillWatch::install(&store, cx);
    cx.set_global(KillWatchGlobal(kill_watch));

    // Lazily-created entity globals — materialize now so later access never
    // races the first coding session.
    let _ = ControlChannels::global(cx);

    // EXP-1294: this machine picks its own `sessions` rows up from the
    // shapes — the one start path.
    crate::session_pickup::install(cx);
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
    // EXP-1457: this install's eNet node rides the same lifecycle.
    crate::enet_wiring::start(account, cx);
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
    // The register's account rows read the profile index.
    let register_dir = auth.data_dir.clone();
    // EXP-1138: the doctor reads the account profiles.
    let doctor_dir = auth.data_dir.clone();
    // Boot pass (EXP-886): apply the device's "Keep session history" window
    // to stored transcripts and resume records. Unlimited, the default, keeps
    // everything; Settings → Sessions re-runs it when the window shrinks.
    {
        let prune_dir = history_dir.clone();
        cx.background_executor()
            .spawn(async move {
                steer::prune_session_history(&prune_dir);
            })
            .detach();
    }
    cx.spawn(async move |cx| {
        // EXP-484: the REPORT is kept, not just the advertisement it
        // derives — `devices.register` also carries the per-agent accounts.
        let report: coding::DoctorReport = cx
            .background_executor()
            .spawn(async move { coding::run_doctor(&settings, &doctor_dir) })
            .await;
        let advertisement: coding::AgentAdvertisement = report.agent_advertisement(&settings2);
        let caps = device_caps(&advertisement);
        // EXP-403: record this machine in the per-user devices registry so the
        // agents UI can show it with an offline "last seen" state (relay
        // presence alone empties on every relay restart). Best-effort — a failed
        // register must never break control-channel start — and registered even with no agents so the
        // UI can explain WHY the machine is not startable. EXP-485: this row
        // is now the ONLY path the agent lists and launch defaults take to the
        // server; the online frame carries neither.
        // NEW-78 / NEW-55: the voice rows the doctor carries.
        let voice = cx.update(|cx| crate::device_readiness::voice_rows(cx));
        register_device(
            Arc::clone(&trpc),
            account_id.clone(),
            device_id.clone(),
            device_label.clone(),
            &advertisement,
            &caps,
            &settings2,
            Some(&report),
            &register_dir,
            voice,
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
            // Non-blocking by contract: flip the flag — the device-sync
            // loop's next 1s tick beats immediately.
            let on_check_in: steer::control_channel::CheckInFn = Arc::new(move || {
                check_in.store(true, std::sync::atomic::Ordering::SeqCst);
            });
            // EXP-773: serve a stored transcript back to the relay. Reading
            // and republishing both happen on the steer runtime — the socket
            // loop only hands the id over.
            let on_history_request: steer::HistoryRequestFn = Arc::new(move |session_id, reply| {
                let tickets: Arc<dyn PublisherTickets> = Arc::new(TrpcPublisherTickets {
                    trpc: Arc::clone(&history_trpc),
                    session_id: session_id.clone(),
                });
                steer::serve_history_request(
                    &history_runtime,
                    tickets,
                    history_dir.clone(),
                    session_id,
                    history_in_flight.clone(),
                    reply,
                );
            });
            let on_history_page: steer::HistoryPageFn = Arc::new(move |ask, reply| {
                steer::serve_history_page(&page_runtime, page_dir.clone(), ask, reply);
            });
            // NEW-55: the relay rings this device for a streamed call; the
            // socket loop only hands it over, the IDE answers on its thread
            // (`voice_call::on_ring`). The task ends with the channel.
            let (ring_tx, ring_rx) = flume::unbounded::<steer::control_channel::CallRing>();
            cx.spawn(async move |cx| {
                while let Ok(ring) = ring_rx.recv_async().await {
                    let _ = cx.update(|cx| etalk_gpui::voice_call::on_ring(ring.call_id, ring.agent_id, cx));
                }
            })
            .detach();
            let on_call_ring: steer::control_channel::CallRingFn = Arc::new(move |ring| {
                let _ = ring_tx.send(ring);
            });
            let control_api: Arc<dyn ControlApi> = Arc::new(TrpcControlApi(trpc));
            let handle = spawn_control_channel(
                &runtime,
                device,
                control_api,
                on_check_in,
                on_history_request,
                on_history_page,
                on_call_ring,
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
    crate::enet_wiring::stop(account_id, cx);
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
        account.id.clone(),
        steer::persistent_device_id(&auth.data_dir),
        api::users::hostname(),
        &desired,
        &caps,
        &settings,
        Some(&report),
        &auth.data_dir,
        crate::device_readiness::voice_rows(cx),
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
/// agent lists and launch defaults take to the server. A failed register must
/// never break control-channel start, so failures are swallowed.
/// EXP-1417: the accounts whose register LANDED in this app process. The
/// first register per account is the launch (the only kind that brings back
/// a machine its owner removed); every later one, a re-dial or an
/// advertisement refresh, leaves a removed machine removed.
static LAUNCH_REGISTERED: std::sync::Mutex<std::collections::BTreeSet<String>> =
    std::sync::Mutex::new(std::collections::BTreeSet::new());

fn launch_registered() -> std::sync::MutexGuard<'static, std::collections::BTreeSet<String>> {
    match LAUNCH_REGISTERED.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

fn register_device(
    trpc: Arc<api::TrpcClient>,
    account_id: String,
    device_id: String,
    device_label: String,
    advertisement: &coding::AgentAdvertisement,
    caps: &[String],
    settings: &coding::Settings,
    report: Option<&coding::DoctorReport>,
    data_dir: &std::path::Path,
    voice: crate::device_readiness::VoiceRows,
    executor: &gpui::BackgroundExecutor,
) {
    let agents = advertisement.agents.clone();
    let unauthed_agents = advertisement.unauthed_agents.clone();
    // EXP-749: which of those speak ACP here. Sent even when empty — the
    // column is what every client reads to say whether an agent can run a
    // session on this machine, and silence there reads as "none".
    let acp_agents = advertisement.acp_agents.clone();
    let caps = caps.to_vec();
    // EXP-484: WHO each installed CLI is signed in as, straight off the
    // doctor probe that produced this advertisement. Skipped when nothing is
    // installed — the server then leaves the column untouched, so an older
    // build's re-register can never blank a row.
    //
    // EXP-951: a shots capture (`EXP_DEV_AGENT_ACCOUNT`) registers DEMO
    // logins, never the capture machine's real ones.
    let agent_accounts = match crate::device_settings::dev_agent_status() {
        Some(demo) => demo.accounts_json(),
        None => report.and_then(|report| {
            let accounts = report.agent_accounts(&coding::agent_accounts::now_iso());
            (!accounts.is_empty())
                .then(|| serde_json::to_value(&accounts).ok())
                .flatten()
        }),
    };
    // EXP-481: the local defaults ride as a first-ever SEED (server-side no-op
    // once the column is set); the device-sync beat reconciles the response
    // copy within one interval, so it is deliberately dropped here.
    let launch_defaults = serde_json::to_value(coding::defaults_wire(settings))
        .expect("defaults serialize cannot fail");
    // EXP-1196: the readiness block, always on register (once the doctor
    // probe has landed). NEW-78: its microphone row probes the OS, so it is
    // added on the background executor.
    let doctor = report.map(|report| coding::device_doctor::current(settings, data_dir, report));
    executor
        .spawn(async move {
            let doctor = doctor.and_then(|mut doctor| {
                crate::device_readiness::add_voice_rows(&mut doctor, voice);
                serde_json::to_value(doctor).ok()
            });
            let launch = !launch_registered().contains(&account_id);
            let result = api::devices::register(
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
                    doctor: doctor.as_ref(),
                    launch,
                    // FEED-154: a desktop row is never shareable, so it
                    // proves no machine key.
                    key: None,
                },
            );
            match result {
                Ok(result) if result.removed => {
                    log::info!("[steer] this machine was removed in Exponential; it stays removed until the app restarts");
                }
                Ok(_) => {
                    launch_registered().insert(account_id);
                }
                Err(_) => {}
            }
        })
        .detach();
}

/// EXP-746 — the account facts the engine needs off the app state.
///
/// REV2-17: the account's `expu_` personal key. It is the redactor's
/// exact-match secret (a codex session carries it in the spawn env, never
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
/// EXP-1294: an ended row is always a stop-now — the row carries no
/// close-out any more.
pub(crate) fn register_kill_feed(session_id: &str, cx: &mut App) -> engine::KillFeed {
    let (kill_tx, kill_rx) = flume::unbounded::<engine::KillReason>();
    let (unwatch_tx, unwatch_rx) = flume::bounded::<()>(1);
    if let Some(kill_watch) = cx.try_global::<KillWatchGlobal>().map(|g| g.0.clone()) {
        let own_device_id = queries::own_device_id(cx);
        kill_watch.update(cx, |watch, cx| {
            watch.watch(
                session_id.to_string(),
                Some(own_device_id),
                Box::new(move |_facts| {
                    let _ = kill_tx.send(engine::KillReason::Now);
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
