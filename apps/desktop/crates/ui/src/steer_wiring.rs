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
//!    launcher on a workspace window (the SAME `coding_flow` path the button
//!    uses, `LaunchOrigin::Relay`).
//!
//! 2. **Publisher — attaches on coding-session launch.**
//!    [`attach_publisher`] is the single call `coding_flow::spawn_into_window`
//!    makes right after `coding` reports `LaunchOutcome::Spawned`. It mints a
//!    publisher ticket over tRPC (never signed locally), builds the §6.14 tee
//!    from the live terminal's Send+Sync handles (`Terminal::writer()` for
//!    remote input inject, `Terminal::term()` for TRUE geometry), attaches the
//!    [`steer::PublisherSink`] to the read-loop, and registers the session for
//!    the kill-switch. Best-effort: a disabled/unreachable relay is a no-op
//! (the session runs fine locally with no remote mirror).
//!
//! 3. **Kill-switch — the own-row Electric watch (§8.8).** Every published
//!    session is registered with the [`sync::KillWatch`]; when its
//!    `coding_sessions` row flips to `ended` (a `steer.killSession` DB write
//!    that reaches us over sync even when the relay is dead), the callback
//!    kills the `claude` child and stops the publisher — the only kill path
//!    that survives a dead relay.
//!
//! Cross-thread discipline: the publisher task runs on the steer tokio runtime,
//! so its hooks must be `Send + Sync`. Two hooks operate on `Send + Sync` `Arc`
//! handles directly (input-inject over the shared PTY writer, geometry over the
//! `TermHandle`); the rest (resize-down, presence, error, relay-kill) marshal
//! onto the gpui foreground through a per-session [`flume`] channel drained by a
//! foreground task, because they touch the gpui-held `Terminal` / registry.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use gpui::{App, AppContext as _, Entity, Global, WeakEntity};
use terminal::{RawSink, TabId, TerminalManager};

use coding::{prepare_launch, LaunchOrigin, Prepared};
use steer::publisher::{pty_writer_input_hook, term_geometry_hook};
use steer::{
    spawn_activity_emitter, spawn_control_channel, ControlApi, ControlChannelHandle, DeviceIdentity,
    EmitterConfig, Presence, PublishSpec, PublisherHandle, PublisherHooks, PublisherTickets,
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
/// on the steer runtime; this hands the issue id to the gpui foreground).
struct RemoteStartGlobal(flume::Sender<String>);
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

    // §8.3 #4: relay `start_session` → foreground launcher.
    let (tx, rx) = flume::unbounded::<String>();
    cx.set_global(RemoteStartGlobal(tx));
    cx.spawn(async move |cx| {
        while let Ok(issue_id) = rx.recv_async().await {
            cx.update(|cx| handle_remote_start(issue_id, cx));
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

    let device = DeviceIdentity {
        device_id: steer::persistent_device_id(&auth.data_dir),
        device_label: api::users::hostname(),
    };
    let inbox = cx.global::<RemoteStartGlobal>().0.clone();
    let on_start: steer::control_channel::StartSessionFn = Arc::new(move |issue_id| {
        let _ = inbox.send(issue_id);
    });
    let control_api: Arc<dyn ControlApi> = Arc::new(TrpcControlApi(trpc));
    let handle = spawn_control_channel(&runtime, device, control_api, on_start);

    let channels = ControlChannels::global(cx);
    let account_id = account.id.clone();
    channels.update(cx, |channels, _| {
        if let Some(previous) = channels.by_account.insert(account_id, handle) {
            previous.stop(); // never accumulate two sockets for one account
        }
    });
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

/// Relay `start_session` → the §7 launcher on a workspace window. The SAME
/// sequence the Start-coding button runs (`coding_flow::build_launch` →
/// `prepare_launch` → `spawn_into_window`), only the [`LaunchOrigin`] differs
/// (§7.1: there is no second, divergent remote-start implementation).
fn handle_remote_start(issue_id: String, cx: &mut App) {
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

    let device_id = steer::persistent_device_id(&AuthContext::global(cx).data_dir);
    let claimant = queries::active_account(cx)
        .map(|account| account.id)
        .unwrap_or_default();
    let origin = LaunchOrigin::Relay {
        device_id,
        claimant,
    };
    let Some((request, deps)) = coding_flow::build_launch(&issue_id, origin, cx) else {
        log::warn!("steer: remote start for {issue_id} ignored — not signed in / not synced");
        return;
    };

    // Pick the first workspace window (one that has a terminal dock). Non-
    // workspace windows (login) can't host a coding tab.
    let target = cx.windows().into_iter().find(|handle| {
        handle
            .update(cx, |_, window, cx| {
                coding_flow::window_terminal_manager(window, cx).is_some()
            })
            .unwrap_or(false)
    });
    let Some(target) = target else {
        log::warn!("steer: remote start for {issue_id} — no workspace window open");
        return;
    };

    cx.spawn(async move |cx| {
        let prepared = cx
            .background_executor()
            .spawn(async move { prepare_launch(&request, &deps) })
            .await;
        let _ = target.update(cx, |_, window, cx| match prepared {
            Ok(Prepared::Ready(prepared)) => {
                // Remote start defaults to NOT private — the phone user
                // initiating it opted the project into live coding; publish if
                // the project is a live feedback board.
                if let Err(message) =
                    coding_flow::spawn_into_window(prepared, issue_id, false, window, cx)
                {
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

// ---------------------------------------------------------------------------
// Publisher registry (§8.4/§8.5) — the sessions this process is publishing
// ---------------------------------------------------------------------------

struct PublisherEntry {
    handle: PublisherHandle,
    /// The tee sink attached to the read-loop (kept for `detach_sink`).
    sink: Arc<dyn RawSink>,
    /// The current remote steerer's display name, if any (§8.5 banner state).
    steerer: Option<String>,
    /// §P7: the activity emitter's run flag when this session publishes a
    /// PUBLIC live stream (feedback board + `publicShowCoding='live'`, not
    /// opted private). `None` when nothing is being published publicly.
    /// Flipping it `false` stops the emitter thread on teardown; its presence
    /// also drives the "LIVE — public" indicator.
    activity_active: Option<Arc<AtomicBool>>,
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
    /// Steerer viewport → resize the LOCAL terminal (§8.4 resize-down).
    Resize(u16, u16),
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
/// Spawned`. Best-effort and non-blocking: a disabled/unreachable relay ends
/// the publisher task quietly and the session keeps running locally.
pub fn attach_publisher(
    session_id: &str,
    issue_id: &str,
    tab: TabId,
    manager: &Entity<TerminalManager>,
    worktree: PathBuf,
    keep_private: bool,
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
    let resize_tx = ui_tx.clone();
    let presence_tx = ui_tx.clone();
    let error_tx = ui_tx.clone();
    let kill_tx = ui_tx.clone();

    let hooks = PublisherHooks {
        // Remote input → the ONE shared PTY writer (Send+Sync Arc, no gpui).
        write_input: pty_writer_input_hook(writer),
        // TRUE geometry for `hello`/re-`hello` (Send+Sync TermHandle, no gpui).
        geometry: term_geometry_hook(term),
        // The rest marshal to the foreground (they touch the gpui-held term).
        resize: Arc::new(move |cols, rows| {
            let _ = resize_tx.send(SteerUiEvent::Resize(cols, rows));
        }),
        kill: Arc::new(move |_signal| {
            let _ = kill_tx.send(SteerUiEvent::Teardown);
        }),
        presence: Arc::new(move |presence| {
            let _ = presence_tx.send(SteerUiEvent::Presence(presence));
        }),
        error: Arc::new(move |message| {
            let _ = error_tx.send(SteerUiEvent::Error(message));
        }),
    };

    let tickets: Arc<dyn PublisherTickets> = Arc::new(TrpcPublisherTickets {
        trpc,
        coding_session_id: session_id.to_string(),
    });
    let spec = PublishSpec {
        session_id: session_id.to_string(),
        issue_id: Some(issue_id.to_string()),
    };
    let handle = steer::publish(&runtime, spec, tickets, hooks);

    // Attach the tee sink to the read-loop (§6.14): from now on every teed
    // chunk fans out to the publisher's bounded channel.
    let sink = handle.raw_sink();
    view.read(cx).session().borrow().attach_sink(sink.clone());

    // §8.4 resize-up: install the §6.10-step-3 observer so a genuine LOCAL grid
    // change (the terminal element's resize path) forwards `resize` up and
    // remote viewers reflow. The observer holds only a cheap resize notifier —
    // `crates/terminal` stays gpui-/steer-free — and the publisher dedups
    // against its last-sent geometry, so a steerer-origin resize can't loop.
    let resize_notifier = handle.resize_notifier();
    view.read(cx)
        .session()
        .borrow_mut()
        .set_resize_observer(Box::new(move |cols, rows| {
            resize_notifier.notify(cols, rows);
        }));

    // §P7: start the PUBLIC live-coding activity emitter when the issue's
    // project is a feedback board with `publicShowCoding='live'` AND the user
    // hasn't opted this session private. The emitter tails the Claude
    // transcript + worktree diffs, redacts, and publishes over the SAME
    // publisher socket (activity frames the relay fans to public viewers only).
    // Best-effort: a relay-disabled instance just drops the sends.
    let publish_live = !keep_private
        && coding_flow::issue_project(issue_id, cx)
            .map(|project| project.is_live_public_coding())
            .unwrap_or(false);
    let activity_active = if publish_live {
        let active = Arc::new(AtomicBool::new(true));
        spawn_activity_emitter(
            EmitterConfig { worktree },
            handle.activity_sender(),
            active.clone(),
        );
        Some(active)
    } else {
        None
    };

    // Register the session (banner state + take-over + teardown).
    let registry = PublisherRegistry::global(cx);
    registry.update(cx, |registry, _| {
        registry.entries.insert(
            session_id.to_string(),
            PublisherEntry {
                handle,
                sink,
                steerer: None,
                activity_active,
            },
        );
    });

    // §8.8 own-row kill-switch: end the session when the synced row flips to
    // `ended` even if the relay is unreachable. The callback is cx-free, so it
    // routes the teardown through the same foreground drain.
    if let Some(kill_watch) = cx.try_global::<KillWatchGlobal>().map(|g| g.0.clone()) {
        let teardown_tx = ui_tx.clone();
        kill_watch.update(cx, |watch, cx| {
            watch.watch(
                session_id.to_string(),
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
        SteerUiEvent::Resize(cols, rows) => {
            resize_local_terminal(manager, tab, cols, rows, cx);
            false
        }
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

/// §8.4 resize-down: apply the steerer's viewport to the local terminal (the
/// terminal no-ops on an unchanged size — kills the resize ping-pong).
fn resize_local_terminal(
    manager: &WeakEntity<TerminalManager>,
    tab: TabId,
    cols: u16,
    rows: u16,
    cx: &mut App,
) {
    let Some(manager) = manager.upgrade() else {
        return;
    };
    let Some(view) = manager.read(cx).tab(tab).map(|tab| tab.view.clone()) else {
        return;
    };
    let _ = view.read(cx).session().borrow_mut().resize(cols, rows);
}

/// End a published session (relay `kill` or own-row Electric kill, §8.4/§8.8):
/// kill the `claude` child (no-op if already exited), stop the publisher, drop
/// the tee sink and the kill-watch, and forget the session.
fn teardown_session(
    session_id: &str,
    manager: &WeakEntity<TerminalManager>,
    tab: TabId,
    cx: &mut App,
) {
    let Some(registry) = PublisherRegistry::global_ref(cx) else {
        return;
    };
    let sink = registry
        .read(cx)
        .entries
        .get(session_id)
        .map(|entry| entry.sink.clone());
    let Some(sink) = sink else {
        return; // already torn down
    };

    // Kill the child + detach the tee on the foreground (the relay never
    // reaches this on the dead-relay path — this is the durable abort).
    if let Some(manager) = manager.upgrade() {
        if let Some(view) = manager.read(cx).tab(tab).map(|tab| tab.view.clone()) {
            let session = view.read(cx).session();
            session.borrow().kill();
            session.borrow().detach_sink(&sink);
            // Drop the §8.4 resize observer so its notifier can't outlive the
            // publisher (send-into-dead-channel is harmless but untidy).
            session.borrow_mut().clear_resize_observer();
        }
    }

    // Stop the publisher (idempotent) and forget the entry. Notify so any
    // §8.5 banner watching this registry clears itself.
    registry.update(cx, |registry, cx| {
        if let Some(entry) = registry.entries.get(session_id) {
            entry.handle.session_ended();
            // §P7: stop the activity emitter thread promptly.
            if let Some(active) = &entry.activity_active {
                active.store(false, Ordering::SeqCst);
            }
        }
        registry.entries.remove(session_id);
        cx.notify();
    });

    // Drop the kill-watch registration so a later row change can't re-fire.
    if let Some(kill_watch) = cx.try_global::<KillWatchGlobal>().map(|g| g.0.clone()) {
        kill_watch.update(cx, |watch, _| watch.unwatch(session_id));
    }
}

// ---------------------------------------------------------------------------
// The §8.5 "Remote steering" surface — consumed by the terminal-tab banner
// ---------------------------------------------------------------------------

/// Whether this session is publishing a PUBLIC live activity stream right now
/// (§P7) — the emitter is running and still active. Drives the "LIVE — public"
/// indicator on the coding-session UI. `false` for private/non-live sessions.
pub fn is_publishing_live(session_id: &str, cx: &App) -> bool {
    PublisherRegistry::global_ref(cx)
        .and_then(|registry| {
            registry
                .read(cx)
                .entries
                .get(session_id)
                .map(|entry| {
                    entry
                        .activity_active
                        .as_ref()
                        .is_some_and(|active| active.load(Ordering::SeqCst))
                })
        })
        .unwrap_or(false)
}

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

/// §6.10 local-resize forward: a genuine local grid change → `resize` up so
/// viewers reflow. The terminal element calls this from its resize path; a
/// no-op for a session we are not publishing.
pub fn notify_local_resize(session_id: &str, cols: u16, rows: u16, cx: &App) {
    if let Some(registry) = PublisherRegistry::global_ref(cx) {
        if let Some(entry) = registry.read(cx).entries.get(session_id) {
            entry.handle.notify_local_resize(cols, rows);
        }
    }
}
