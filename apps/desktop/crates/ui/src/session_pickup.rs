//! EXP-1294 (Threads v2) — the desktop's half of "the database is the bus":
//! this machine hosts every `sessions` row whose `device_id` is its own, and
//! picks queued rows and new person messages up from the synced shapes.
//!
//! The decisions are `coding::pickup`'s ([`coding::pickup::plan`], PURE): this
//! host snapshots the `sessions` and `messages` collections plus what runs
//! here, and executes the steps it gets back:
//!
//! - `Launch` → [`coding::pickup::launch_queued`] on the background executor
//!   (claim, brief, prepare); its `LaunchFn` hops back to the foreground and
//!   starts the engine through [`crate::coding_flow::host_prepared`].
//! - `Deliver` / `Resume` → [`coding::pickup::deliver_messages`] (a session
//!   not live here is resumed natively, on the same row).
//! - `Kill` / `Answer` / `Revoke` → the hosted engine, straight away.
//!
//! The engine's own worker consumes delivered messages and reports back, so
//! nothing here writes a row.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use gpui::{App, Global};

use coding::pickup::{self, LaunchQueued, LiveEngine, PickupAction, PickupDeps, PickupInput};
use domain::rows::{MessageRow, SessionRow};
use sync::Store;

use crate::coding_flow::{self, LocalSessions};
use crate::queries;

/// One `LaunchFn` call handed to the foreground: the prepared launch and
/// where its answer goes.
type LaunchRequest = (
    coding::PreparedLaunch,
    flume::Sender<Result<Arc<dyn LiveEngine>, String>>,
);

/// The host's foreground state.
struct PickupHost {
    /// The foreground end of every `LaunchFn` (see [`launch_fn`]).
    launches: flume::Sender<LaunchRequest>,
    /// The pickup's registry of the sessions live here, one for the
    /// process (every pass hands the same one to `coding::pickup`).
    live: Arc<pickup::LiveRegistry>,
    /// Sessions whose launch or delivery is on the background executor.
    in_flight: HashSet<String>,
    /// Messages handed straight to a live engine whose `consumed_at` has not
    /// synced back yet — never delivered twice.
    delivered: HashSet<String>,
    /// A pass is already scheduled for the next effect cycle.
    scheduled: bool,
}

impl Global for PickupHost {}

/// Stand the host up (once, from [`crate::steer_wiring::install`], after the
/// `Store` exists): the launch inbox plus the collection watches that
/// schedule a pass.
pub(crate) fn install(cx: &mut App) {
    if cx.has_global::<PickupHost>() {
        return;
    }
    let (launches, inbox) = flume::unbounded::<LaunchRequest>();
    cx.set_global(PickupHost {
        launches,
        live: Arc::new(pickup::LiveRegistry::new()),
        in_flight: HashSet::new(),
        delivered: HashSet::new(),
        scheduled: false,
    });
    cx.spawn(async move |cx| {
        while let Ok((prepared, reply)) = inbox.recv_async().await {
            let hosted = cx.update(|cx| coding_flow::host_prepared(prepared, cx));
            let _ = reply.send(hosted.map(|session| -> Arc<dyn LiveEngine> {
                Arc::new(HostedEngine(session))
            }));
        }
    })
    .detach();
    let Some(collections) = Store::try_global(cx).map(|store| store.collections().clone()) else {
        return;
    };
    cx.observe(&collections.sessions, |_, cx| schedule(cx)).detach();
    cx.observe(&collections.messages, |_, cx| schedule(cx)).detach();
    let local = LocalSessions::global(cx);
    cx.observe(&local, |_, cx| schedule(cx)).detach();
    schedule(cx);
}

/// Run one pass on the next effect cycle (coalesced).
fn schedule(cx: &mut App) {
    let Some(host) = cx.try_global::<PickupHost>() else {
        return;
    };
    if host.scheduled {
        return;
    }
    cx.global_mut::<PickupHost>().scheduled = true;
    cx.defer(|cx| {
        cx.global_mut::<PickupHost>().scheduled = false;
        run_pass(cx);
    });
}

/// One pass: snapshot, plan, execute.
fn run_pass(cx: &mut App) {
    if queries::active_account(cx).is_none() {
        return;
    }
    let Some(store) = Store::try_global(cx) else {
        return;
    };
    let collections = store.collections().clone();
    let device_id = queries::own_device_id(cx);
    // Only this machine's sessions (and their messages) ever reach the plan.
    let sessions: Vec<SessionRow> = collections
        .sessions
        .read(cx)
        .iter()
        .filter(|row| row.device_id.as_deref() == Some(device_id.as_str()))
        .cloned()
        .collect();
    let hosted: HashSet<&str> = sessions.iter().map(|row| row.id.as_str()).collect();
    let messages: Vec<MessageRow> = collections
        .messages
        .read(cx)
        .iter()
        .filter(|row| hosted.contains(row.session_id.as_str()))
        .cloned()
        .collect();
    // A consumed message has left the queue: forget it.
    let queued: HashSet<&str> = messages
        .iter()
        .filter(|row| row.is_queued())
        .map(|row| row.id.as_str())
        .collect();
    cx.global_mut::<PickupHost>()
        .delivered
        .retain(|id| queued.contains(id.as_str()));
    let live: HashSet<String> = LocalSessions::global(cx)
        .read(cx)
        .session_ids()
        .into_iter()
        .collect();
    let in_flight = cx.global::<PickupHost>().in_flight.clone();
    let actions = pickup::plan(PickupInput {
        device_id: &device_id,
        sessions: &sessions,
        messages: &messages,
        live: &live,
        in_flight: &in_flight,
    });
    if actions.is_empty() {
        return;
    }
    let Some(deps) = pickup_deps(&device_id, &sessions, cx) else {
        return;
    };
    let deps = Arc::new(deps);
    for action in actions {
        match action {
            PickupAction::Launch { session_id } => {
                let Some(row) = sessions.iter().find(|row| row.id == session_id).cloned() else {
                    continue;
                };
                spawn_step(session_id, cx, {
                    let deps = Arc::clone(&deps);
                    move || match pickup::launch_queued(&row, &deps) {
                        Ok(LaunchQueued::Launched { session_id }) => {
                            log::info!("[pickup] launched {session_id}");
                        }
                        Ok(LaunchQueued::Skipped { reason }) => {
                            log::info!("[pickup] {} skipped: {reason}", row.id);
                        }
                        Ok(LaunchQueued::Disabled { reason }) => {
                            log::warn!("[pickup] {} disabled: {reason}", row.id);
                        }
                        Err(err) => log::warn!("[pickup] launching {} failed: {err}", row.id),
                    }
                });
            }
            PickupAction::Deliver {
                session_id,
                message_ids,
            } => {
                let rows: Vec<MessageRow> = messages
                    .iter()
                    .filter(|row| message_ids.contains(&row.id))
                    .cloned()
                    .collect();
                // A session live here takes its messages straight into its
                // engine; one that is not is resumed natively.
                match hosted_engine(&session_id, cx) {
                    Some(engine) => {
                        for row in rows {
                            if cx.global_mut::<PickupHost>().delivered.insert(row.id.clone()) {
                                engine.deliver(row.id.clone(), row.body_text().to_string());
                            }
                        }
                    }
                    None => deliver(session_id, rows, &deps, cx),
                }
            }
            PickupAction::Resume { session_id } => {
                let rows: Vec<MessageRow> = messages
                    .iter()
                    .filter(|row| row.session_id == session_id && row.is_queued())
                    .cloned()
                    .collect();
                deliver(session_id, rows, &deps, cx);
            }
            PickupAction::Revoke {
                session_id,
                message_id,
            } => {
                if let Some(engine) = hosted_engine(&session_id, cx) {
                    engine.unqueue(&message_id);
                }
            }
            PickupAction::Answer {
                session_id,
                message_id,
                card,
            } => {
                if let Some(engine) = hosted_engine(&session_id, cx) {
                    engine.answer_card(&message_id, &card);
                }
            }
            PickupAction::Kill { session_id } => {
                if let Some(engine) = hosted_engine(&session_id, cx) {
                    engine.kill("ended");
                }
            }
        }
    }
}

/// `Deliver` / `Resume`: hand `rows` to the session (resumed natively when
/// it is not live here).
fn deliver(session_id: String, rows: Vec<MessageRow>, deps: &Arc<PickupDeps>, cx: &mut App) {
    let deps = Arc::clone(deps);
    let id = session_id.clone();
    spawn_step(session_id, cx, move || {
        match pickup::deliver_messages(&id, &rows, &deps) {
            Ok(delivered) => log::info!("[pickup] delivered {} message(s) to {id}", delivered.len()),
            Err(err) => log::warn!("[pickup] delivering to {id} failed: {err}"),
        }
    });
}

/// Run one blocking pickup step on the background executor with
/// `session_id` marked in flight, then plan again.
fn spawn_step(session_id: String, cx: &mut App, step: impl FnOnce() + Send + 'static) {
    cx.global_mut::<PickupHost>()
        .in_flight
        .insert(session_id.clone());
    cx.spawn(async move |cx| {
        cx.background_executor().spawn(async move { step() }).await;
        let _ = cx.update(|cx| {
            cx.global_mut::<PickupHost>().in_flight.remove(&session_id);
            schedule(cx);
        });
    })
    .detach();
}

/// The engine hosting `session_id` here, if any.
fn hosted_engine(session_id: &str, cx: &App) -> Option<HostedEngine> {
    let local = LocalSessions::global_ref(cx)?;
    let session = local.read(cx).session_for_id(session_id)?;
    Some(HostedEngine(session.host.session.clone()))
}

/// What the pickup needs from this app, snapshotted on the foreground. The
/// root → identifiers map is captured whole: the pickup asks it on the
/// background executor, where the collections are unreachable.
fn pickup_deps(device_id: &str, sessions: &[SessionRow], cx: &mut App) -> Option<PickupDeps> {
    let coding = coding_flow::build_coding_deps(cx)?;
    let settings = coding.settings.clone();
    let mut identifiers: HashMap<String, Vec<String>> = HashMap::new();
    for row in sessions {
        let root_id = row.root_id_or_self().to_string();
        if !identifiers.contains_key(&root_id) {
            let idents = crate::session_data::root_issue_identifiers(&root_id, cx);
            identifiers.insert(root_id, idents);
        }
    }
    Some(PickupDeps {
        coding: Arc::new(coding),
        device_id: device_id.to_string(),
        device_label: coding::default_device_label(),
        options: coding::LaunchOptions::defaults_for(&settings, settings.default_agent),
        live: Arc::clone(&cx.global::<PickupHost>().live),
        launch: launch_fn(cx),
        issue_identifiers: Arc::new(move |root_id: &str| {
            identifiers.get(root_id).cloned().unwrap_or_default()
        }),
    })
}

/// The pickup's spawn: called on the background executor, it hands the
/// prepared launch to the foreground inbox ([`install`]) and waits for the
/// engine. Never call it on the foreground — it blocks until the foreground
/// answers.
fn launch_fn(cx: &App) -> pickup::LaunchFn {
    let launches = cx.global::<PickupHost>().launches.clone();
    Arc::new(move |prepared| {
        let (reply, answer) = flume::bounded(1);
        launches
            .send((prepared, reply))
            .map_err(|_| "The app is shutting down.".to_string())?;
        answer
            .recv()
            .map_err(|_| "The app is shutting down.".to_string())?
    })
}

/// EXP-1294: a hosted engine as the pickup drives it. Stands in for
/// #EXP-1292's `impl LiveEngine for engine::EngineSession` until that lands.
struct HostedEngine(engine::EngineSession);

impl LiveEngine for HostedEngine {
    fn deliver(&self, _message_id: String, text: String) {
        self.0.steer(text);
    }

    fn unqueue(&self, message_id: &str) {
        self.0.unqueue(message_id);
    }

    fn answer_card(&self, question_id: &str, card: &serde_json::Value) {
        let text = |key: &str| card.get(key).and_then(serde_json::Value::as_str);
        let keys = text("answer")
            .or_else(|| text("decision"))
            .map(|key| vec![key.to_string()])
            .unwrap_or_default();
        self.0.answer(steer::RemoteAnswer {
            question_id: question_id.to_string(),
            ask_id: None,
            keys,
            text: text("feedback").map(str::to_string),
        });
    }

    fn open_cards(&self) -> HashMap<String, String> {
        HashMap::new()
    }

    fn kill(&self, outcome: &'static str) {
        self.0.kill(outcome);
    }

    fn is_done(&self) -> bool {
        self.0.is_done()
    }
}
