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
    prepare_with_hooks, BatchIssueSpec, BatchLaunchRequest, LaunchOptions, LaunchOrigin, Prepared,
    PrepareRequest, RepoGroup,
};
use steer::publisher::pty_writer_input_hook;
use steer::{
    spawn_activity_emitter, spawn_control_channel, AnswerLink, ControlApi, ControlChannelHandle,
    DeviceIdentity, EmitterConfig, HookEvent, HookServer, Presence, PublishSpec, PublisherHandle,
    PublisherHooks, PublisherTickets, Steering, SteerRuntime, TrpcControlApi, TrpcPublisherTickets,
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

fn runtime(cx: &App) -> Option<Arc<SteerRuntime>> {
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
                log::warn!("steer: hooks sidecar failed to bind: {err}");
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
    /// dropping it unsubscribes on the next delivery.
    fn subscribe(&self, worktree: &Path) -> flume::Receiver<HookEvent> {
        let (tx, rx) = flume::unbounded();
        let mut subscribers = match self.subscribers.lock() {
            Ok(subscribers) => subscribers,
            Err(poisoned) => poisoned.into_inner(),
        };
        subscribers.retain(|subscriber| !subscriber.tx.is_disconnected());
        subscribers.push(HookSubscriber {
            worktree: canonical(worktree),
            tx,
            bound: HashSet::new(),
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
// Control channel (§8.3) — per account, starts with the app session
// ---------------------------------------------------------------------------

/// Per-account [`ControlChannelHandle`]s (multi-window shares the one channel
/// per account — it is account-scoped, not window-scoped, §8.3).
#[derive(Default)]
struct ControlChannels {
    by_account: HashMap<String, ControlChannelHandle>,
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
    // the channel starts on the foreground with the result. A settings
    // change re-advertises on the next channel restart (sign-in / account
    // switch / relay reconnect cycles re-run this whole function).
    let settings = coding_flow::CodingHub::global(cx).read(cx).settings.clone();
    let device_id = steer::persistent_device_id(&auth.data_dir);
    let device_label = api::users::hostname();
    let inbox = cx.global::<RemoteStartGlobal>().0.clone();
    let account_id = account.id.clone();
    cx.spawn(async move |cx| {
        let agents: Vec<String> = cx
            .background_executor()
            .spawn(async move {
                coding::run_doctor(&settings)
                    .installed_agents()
                    .into_iter()
                    .map(|agent| agent.id().to_string())
                    .collect()
            })
            .await;
        // EXP-253/EXP-257: the actions capabilities — advertised when ANY
        // agent is usable (action runs stopped being Claude-only), plus
        // `action-inputs` (this build understands builtin + inputs-carrying
        // starts) and `fix-conflicts` (EXP-259: this build launches the
        // "Fix merge conflicts" builtin — a pre-EXP-259 desktop would treat
        // its id as a real action and fail the fetch). Remote clients
        // strictly gate action starts on these, so an incapable desktop is
        // never targeted.
        let caps: Vec<String> = if agents.is_empty() {
            Vec::new()
        } else {
            vec![
                "actions".to_string(),
                "action-inputs".to_string(),
                "fix-conflicts".to_string(),
            ]
        };
        let _ = cx.update(|cx| {
            // The probe raced a sign-out/switch: starting a socket for a
            // no-longer-active account would leak it past its stop call.
            if queries::active_account(cx).map(|account| account.id)
                != Some(account_id.clone())
            {
                return;
            }
            let device = DeviceIdentity {
                device_id,
                device_label,
                agents,
                caps,
            };
            let on_start: steer::control_channel::StartSessionFn = Arc::new(move |start| {
                let _ = inbox.send(start);
            });
            let control_api: Arc<dyn ControlApi> = Arc::new(TrpcControlApi(trpc));
            let handle = spawn_control_channel(&runtime, device, control_api, on_start);

            let channels = ControlChannels::global(cx);
            channels.update(cx, |channels, _| {
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
        });
    }
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
    }
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
    // No dedup (batch precedent): each run is its own session; concurrent
    // repo-backed runs share the trunk cwd exactly like two shell tabs.
    let settings = coding_flow::CodingHub::global(cx).read(cx).settings.clone();
    let options = LaunchOptions::remote(
        &settings,
        start.agent.as_deref(),
        start.model.as_deref(),
        start.effort.as_deref(),
        start.ultracode,
        start.plan_mode,
        start.skip_permissions,
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
            origin: relay_origin(cx),
            inputs,
            target: None,
            activate_app: true,
        },
        cx,
    );
}

/// The §08 relay [`LaunchOrigin`] for the signed-in account: the persistent
/// device id + the active account id (the session's audit surface, §7.1 — not
/// a branch key).
fn relay_origin(cx: &App) -> LaunchOrigin {
    let device_id = steer::persistent_device_id(&AuthContext::global(cx).data_dir);
    let claimant = queries::active_account(cx)
        .map(|account| account.id)
        .unwrap_or_default();
    LaunchOrigin::Relay {
        device_id,
        claimant,
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

    let origin = relay_origin(cx);
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
        start.skip_permissions,
    );
    // Remote resume is deferred (EXP-202) — relay starts are always fresh.
    let Some((request, deps)) = coding_flow::build_launch(&issue_id, origin, options, false, cx)
    else {
        log::warn!("steer: remote start for {issue_id} ignored — not signed in / not synced");
        return;
    };

    let Some(target) = find_team_window(cx) else {
        log::warn!("steer: remote start for {issue_id} — no shell window open");
        return;
    };

    let hooks = hook_setup(cx);
    cx.spawn(async move |cx| {
        let prepared = cx
            .background_executor()
            .spawn(async move {
                prepare_with_hooks(&PrepareRequest::Issue(request), &deps, hooks.as_ref())
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
    // No dedup (unlike the issue branch): each batch run mints a fresh
    // `exp/batch-<id8>` branch, so there is never a worktree collision to
    // guard against.

    // Resolve the checked issues from sync. Unknown ids are skipped; a
    // resolved issue whose board is outside the claimed team aborts the
    // WHOLE batch — a remote client must never steer this desktop into coding
    // issues from another team than the one it claimed.
    let issues: Vec<BatchIssueSpec> = {
        let store = Store::global(cx);
        let issues_coll = store.collections().issues.read(cx);
        let boards_coll = store.collections().boards.read(cx);
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
        start.skip_permissions,
    );

    // Same field construction the dialog's `batch_request` uses (device_label
    // from `coding::default_device_label()`, a fresh `coding::new_batch_id()`).
    let batch_id = coding::new_batch_id();
    let request = BatchLaunchRequest {
        batch_id: batch_id.clone(),
        team_id,
        repo: RepoGroup {
            repository_id: repo.repository_id,
            full_name: repo.full_name,
            default_branch: repo.default_branch,
        },
        issues,
        device_label: coding::default_device_label(),
        origin: relay_origin(cx),
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
    cx.spawn(async move |cx| {
        let prepared = cx
            .background_executor()
            .spawn(async move {
                prepare_with_hooks(&PrepareRequest::Batch(request), &deps, hooks.as_ref())
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
    /// The current remote steerer's display name, if any (§8.5 banner state).
    steerer: Option<String>,
    /// §P7: the activity emitter's run flag (members-only activity channel);
    /// flipping it `false` stops the thread on teardown.
    activity_active: Arc<AtomicBool>,
}

/// Session-keyed publisher handles — parallels `coding_flow::LocalSessions`
/// but holds the steer side (the publisher task handle + banner state).
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
    /// A `presence` broadcast → update the §8.5 banner state.
    Presence(Presence),
    /// A surfaced publisher error (clock skew, repeated rejects — §8.7).
    Error(String),
    /// End the session: a relay `kill` frame OR the own-row Electric kill
    /// (§8.4/§8.8). Kills the child + stops the publisher; drains stop after.
    Teardown,
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
    cx: &mut App,
) {
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
    let presence_tx = ui_tx.clone();
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

    let hooks = PublisherHooks {
        write_input: write_input.clone(),
        // The rest marshal to the foreground (they touch the gpui-held term).
        kill: Arc::new(move |_signal| {
            let _ = kill_tx.send(SteerUiEvent::Teardown);
        }),
        presence: Arc::new(move |presence| {
            let _ = presence_tx.send(SteerUiEvent::Presence(presence));
        }),
        error: Arc::new(move |message| {
            let _ = error_tx.send(SteerUiEvent::Error(message));
        }),
        answers: Some(answer_link.clone()),
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
    // EXP-214: forward the emitter's picker-pending flips to the synced
    // `coding_sessions.needs_input` column so every client can badge
    // "Needs input". Fire-and-forget from the emitter thread (blocking
    // HTTP is fine there); a swept/ended row reports `updated: false`.
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
    // came up; the emitter then runs grid-only, exactly as before.
    let hook_events = cx
        .try_global::<HookSidecarGlobal>()
        .map(|global| global.0.subscribe(&worktree));
    spawn_activity_emitter(
        EmitterConfig {
            worktree,
            extra_secrets,
            // The live grid: the emitter watches it to confirm pickers the
            // transcript can't show while PENDING (EXP-150) and to choreograph
            // a remote answer's keystrokes (EXP-249).
            term: Some(term),
            on_needs_input: Some(Arc::new(move |pending| {
                if let Err(err) = api::coding_sessions::set_needs_input(
                    &needs_input_trpc,
                    &needs_input_session,
                    pending,
                ) {
                    log::debug!("steer: setNeedsInput({pending}) failed: {err}");
                }
            })),
            hooks: hook_events,
            steering: Some(Steering {
                answers,
                link: answer_link,
                write_input,
            }),
        },
        handle.activity_sender(),
        activity_active.clone(),
    );

    // Register the session (banner state + take-over + teardown).
    let registry = PublisherRegistry::global(cx);
    registry.update(cx, |registry, _| {
        registry.entries.insert(
            session_id.to_string(),
            PublisherEntry {
                handle,
                steerer: None,
                activity_active,
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
                Box::new(move || {
                    let _ = teardown_tx.send(SteerUiEvent::Teardown);
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
        SteerUiEvent::Presence(presence) => {
            let name = presence.steerer_id.as_ref().and_then(|steerer_id| {
                presence
                    .viewers
                    .iter()
                    .find(|viewer| &viewer.user_id == steerer_id)
                    .map(|viewer| viewer.name.clone())
            });
            if let Some(registry) = PublisherRegistry::global_ref(cx) {
                registry.update(cx, |registry, cx| {
                    if let Some(entry) = registry.entries.get_mut(session_id) {
                        // Notify only on a real change so the §8.5 banner
                        // repaints exactly when the remote steerer flips.
                        if entry.steerer != name {
                            entry.steerer = name;
                            cx.notify();
                        }
                    }
                });
            }
            false
        }
        SteerUiEvent::Error(message) => {
            log::warn!("steer publisher [{session_id}]: {message}");
            false
        }
        SteerUiEvent::Teardown => {
            teardown_session(session_id, manager, tab, cx);
            true
        }
    }
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
                // Stop the publisher (idempotent) and notify so any §8.5
                // banner watching this registry clears itself.
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

// ---------------------------------------------------------------------------
// The §8.5 "Remote steering" surface — consumed by the terminal-tab banner
// ---------------------------------------------------------------------------

/// The remote steerer's display name for a published session, if one is
/// steering right now (§8.5). Drives the terminal-tab "Remote steering — {name}"
/// banner; the LOCAL user is never gated (they type straight to the PTY).
pub fn remote_steerer(session_id: &str, cx: &App) -> Option<String> {
    PublisherRegistry::global_ref(cx)?
        .read(cx)
        .entries
        .get(session_id)
        .and_then(|entry| entry.steerer.clone())
}

/// The §8.5 banner's per-tab entry point: resolve the coding session shown in
/// `tab` and, if a remote viewer holds the claim right now, return its
/// `(session_id, name)` — the session id keys the "Take over" button. `None`
/// when the tab is not a live coding session this process is publishing, or
/// nobody remote is steering it. The LOCAL user is never gated (§8.5).
pub fn remote_steerer_for_tab(tab: TabId, cx: &App) -> Option<(String, String)> {
    let sessions = coding_flow::LocalSessions::global_ref(cx)?;
    let session_id = sessions.read(cx).session_id_for_tab(tab)?.to_string();
    let name = remote_steerer(&session_id, cx)?;
    Some((session_id, name))
}

/// Observe the §8.5 publisher registry so a surface (the terminal-dock banner)
/// repaints when a `presence` frame changes the remote steerer. Returns `None`
/// (no subscription) when steer isn't installed — e.g. headless tests — so the
/// caller degrades to a static, never-shown banner. The registry global is
/// materialized in [`install`], before any window/dock exists.
pub fn observe_steer_presence<T: 'static>(
    cx: &mut gpui::Context<T>,
    mut on_change: impl FnMut(&mut T, &mut gpui::Context<T>) + 'static,
) -> Option<gpui::Subscription> {
    let registry = PublisherRegistry::global_ref(cx)?;
    Some(cx.observe(&registry, move |this, _registry, cx| on_change(this, cx)))
}

/// The §8.5 "Take over" button: send a publisher `claim`, which the relay's
/// publisher-branch turns into `publisherTakeover` (force-clears the remote
/// steerer). A no-op for a session we are not publishing.
pub fn take_over(session_id: &str, cx: &App) {
    if let Some(registry) = PublisherRegistry::global_ref(cx) {
        if let Some(entry) = registry.read(cx).entries.get(session_id) {
            entry.handle.take_over();
        }
    }
}


