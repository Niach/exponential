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
use std::time::Duration;

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
    /// `(session_id, message_id)` handed straight to a live engine whose
    /// `consumed_at` has not synced back yet — never delivered twice to the
    /// same engine. A session's entries drop when it stops being live here,
    /// so a fresh engine gets its still-queued messages again.
    delivered: HashSet<(String, String)>,
    /// Sessions whose last step changed nothing (a refused or foreign-held
    /// claim, a failed launch or resume), keyed to the [`fingerprint`] they
    /// stalled at: the plan's `Launch`/`Resume` for them waits until the row
    /// or its queue moves (or the retry timer of a failure clears it).
    /// Without it every finished step re-plans the same step at once.
    stalled: HashMap<String, String>,
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
        stalled: HashMap::new(),
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
    let live: HashSet<String> = LocalSessions::global(cx)
        .read(cx)
        .session_ids()
        .into_iter()
        .collect();
    // A consumed message has left the queue, and a session no longer live
    // here took its engine (and what it was handed) with it: forget both.
    let queued: HashSet<&str> = messages
        .iter()
        .filter(|row| row.is_queued())
        .map(|row| row.id.as_str())
        .collect();
    let fingerprints: HashMap<&str, String> = sessions
        .iter()
        .map(|row| (row.id.as_str(), fingerprint(row, &messages)))
        .collect();
    {
        let host = cx.global_mut::<PickupHost>();
        host.delivered.retain(|(session_id, message_id)| {
            live.contains(session_id) && queued.contains(message_id.as_str())
        });
        // A stall lasts only while nothing about the session moved.
        host.stalled.retain(|session_id, at| {
            fingerprints.get(session_id.as_str()) == Some(&*at)
        });
    }
    let in_flight = cx.global::<PickupHost>().in_flight.clone();
    let stalled = cx.global::<PickupHost>().stalled.clone();
    let is_stalled = |session_id: &str| stalled.contains_key(session_id);
    let actions: Vec<PickupAction> = pickup::plan(PickupInput {
        device_id: &device_id,
        sessions: &sessions,
        messages: &messages,
        live: &live,
        in_flight: &in_flight,
    })
    .into_iter()
    .filter(|action| match action {
        PickupAction::Launch { session_id } | PickupAction::Resume { session_id } => {
            !is_stalled(session_id)
        }
        PickupAction::Deliver { session_id, .. } => {
            live.contains(session_id) || !is_stalled(session_id)
        }
        _ => true,
    })
    .collect();
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
                let at = fingerprints.get(session_id.as_str()).cloned().unwrap_or_default();
                spawn_step(session_id, at, cx, {
                    let deps = Arc::clone(&deps);
                    move || match pickup::launch_queued(&row, &deps) {
                        Ok(LaunchQueued::Launched { session_id }) => {
                            log::info!("[pickup] launched {session_id}");
                            StepOutcome::Progressed
                        }
                        Ok(LaunchQueued::Skipped { reason }) => {
                            log::info!("[pickup] {} skipped: {reason}", row.id);
                            StepOutcome::Stalled
                        }
                        Ok(LaunchQueued::Disabled { reason }) => {
                            log::warn!("[pickup] {} disabled: {reason}", row.id);
                            StepOutcome::Stalled
                        }
                        Err(err) => {
                            log::warn!("[pickup] launching {} failed: {err}", row.id);
                            StepOutcome::Failed
                        }
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
                            let key = (session_id.clone(), row.id.clone());
                            if cx.global_mut::<PickupHost>().delivered.insert(key) {
                                engine.deliver(row.id.clone(), row.body_text().to_string());
                            }
                        }
                    }
                    None => {
                        let at = fingerprints.get(session_id.as_str()).cloned().unwrap_or_default();
                        deliver(session_id, at, rows, &deps, cx)
                    }
                }
            }
            PickupAction::Resume { session_id } => {
                let rows: Vec<MessageRow> = messages
                    .iter()
                    .filter(|row| row.session_id == session_id && row.is_queued())
                    .cloned()
                    .collect();
                let at = fingerprints.get(session_id.as_str()).cloned().unwrap_or_default();
                deliver(session_id, at, rows, &deps, cx);
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
fn deliver(
    session_id: String,
    at: String,
    rows: Vec<MessageRow>,
    deps: &Arc<PickupDeps>,
    cx: &mut App,
) {
    let deps = Arc::clone(deps);
    let id = session_id.clone();
    spawn_step(session_id, at, cx, move || {
        match pickup::deliver_messages(&id, &rows, &deps) {
            Ok(delivered) if delivered.is_empty() => {
                log::info!("[pickup] nothing delivered to {id}");
                StepOutcome::Stalled
            }
            Ok(delivered) => {
                log::info!("[pickup] delivered {} message(s) to {id}", delivered.len());
                StepOutcome::Progressed
            }
            Err(err) => {
                log::warn!("[pickup] delivering to {id} failed: {err}");
                StepOutcome::Failed
            }
        }
    });
}

/// How a pickup step ended: whether the next pass may plan the same step
/// for that session straight away.
enum StepOutcome {
    /// It moved the session along (the rows will follow).
    Progressed,
    /// It changed nothing and retrying unchanged would change nothing either
    /// (a refused or foreign-held claim): wait for the row to move.
    Stalled,
    /// It failed: wait for the row to move, or [`STEP_RETRY`].
    Failed,
}

/// How long a FAILED step waits before the same step is planned again.
const STEP_RETRY: Duration = Duration::from_secs(30);

/// What a stall waits on: the row (its `updated_at` and status) plus the ids
/// of its queued messages — any change re-opens the session to the plan.
fn fingerprint(row: &SessionRow, messages: &[MessageRow]) -> String {
    let mut at = format!(
        "{}|{}",
        row.updated_at.as_deref().unwrap_or_default(),
        row.status.as_deref().unwrap_or_default()
    );
    for message in messages {
        if message.session_id == row.id && message.is_queued() {
            at.push('|');
            at.push_str(&message.id);
        }
    }
    at
}

/// Run one blocking pickup step on the background executor with
/// `session_id` marked in flight, then plan again. A step that changed
/// nothing parks the session at `at` (its [`fingerprint`] when planned).
fn spawn_step(
    session_id: String,
    at: String,
    cx: &mut App,
    step: impl FnOnce() -> StepOutcome + Send + 'static,
) {
    cx.global_mut::<PickupHost>()
        .in_flight
        .insert(session_id.clone());
    cx.spawn(async move |cx| {
        let outcome = cx.background_executor().spawn(async move { step() }).await;
        let failed = matches!(outcome, StepOutcome::Failed);
        let _ = cx.update(|cx| {
            let host = cx.global_mut::<PickupHost>();
            host.in_flight.remove(&session_id);
            if !matches!(outcome, StepOutcome::Progressed) {
                host.stalled.insert(session_id.clone(), at.clone());
            }
            schedule(cx);
        });
        if failed {
            cx.background_executor().timer(STEP_RETRY).await;
            let _ = cx.update(|cx| {
                let host = cx.global_mut::<PickupHost>();
                if host.stalled.get(&session_id) == Some(&at) {
                    host.stalled.remove(&session_id);
                    schedule(cx);
                }
            });
        }
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
    /// A local session has no publisher in front of it, so a `/` command
    /// the agent knows runs as a command, never as prompt text (EXP-746).
    fn deliver(&self, _message_id: String, text: String) {
        let agent = match self.0.agent() {
            coding::CodingAgent::Claude => steer::SessionAgent::Claude,
            coding::CodingAgent::Codex => steer::SessionAgent::Codex,
        };
        match steer::commands::parse_command(&text, agent) {
            Some(parsed) => self.0.run_command(parsed.command.name, &parsed.args),
            None => self.0.steer(text),
        }
    }

    fn unqueue(&self, message_id: &str) {
        self.0.unqueue(message_id);
    }

    /// The answered `messages` card → the engine's own open question: the
    /// card carries the option LABEL (`answer`) or the plan `decision`, the
    /// engine wants its question id and option KEYS, so both are looked up
    /// on the feed's open question cards.
    fn answer_card(&self, _message_id: &str, card: &serde_json::Value) {
        let open = open_questions(&self.0.subscribe());
        if let Some(answer) = engine_answer(card, &open) {
            self.0.answer(answer);
        }
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

/// One open question card on the engine's feed.
#[derive(Debug, Clone, PartialEq)]
struct OpenQuestion {
    id: String,
    ask_id: Option<String>,
    text: String,
    plan: bool,
    options: Vec<(String, String)>,
}

/// The engine's still-open question cards, oldest first, off a fresh feed
/// subscription (its backlog replays synchronously).
fn open_questions(feed: &flume::Receiver<engine::LocalFeedEvent>) -> Vec<OpenQuestion> {
    let mut open: Vec<OpenQuestion> = Vec::new();
    for event in feed.try_iter() {
        let engine::LocalFeedEvent::Activity { event, .. } = event else {
            continue;
        };
        match event {
            steer::ActivityEvent::Question {
                text,
                options,
                plan_mode,
                id,
                ask_id,
                ..
            } => {
                open.retain(|question| question.id != id);
                open.push(OpenQuestion {
                    id,
                    ask_id,
                    text,
                    plan: plan_mode == Some(true),
                    options: options.into_iter().map(|option| (option.label, option.key)).collect(),
                });
            }
            steer::ActivityEvent::QuestionResolved { id, ask_id, .. } => match (id, ask_id) {
                (Some(id), _) => open.retain(|question| question.id != id),
                (None, Some(ask_id)) => {
                    open.retain(|question| question.ask_id.as_deref() != Some(ask_id.as_str()))
                }
                (None, None) => {}
            },
            _ => {}
        }
    }
    open
}

/// Map an answered `messages` card onto the engine's open card: a question
/// matches by its text (else the newest open question), its `answer` label
/// picks the option key; a plan's `approved` picks the first non-reject
/// option, `rejected` the reject one (its `feedback` rides as the text).
fn engine_answer(card: &serde_json::Value, open: &[OpenQuestion]) -> Option<steer::RemoteAnswer> {
    let text = |key: &str| card.get(key).and_then(serde_json::Value::as_str);
    let is_reject = |key: &str| {
        let key = key.to_ascii_lowercase();
        key.contains("reject") || key.contains("deny")
    };
    match text("kind")? {
        "question" => {
            let label = text("answer")?;
            let candidates = open.iter().filter(|question| !question.plan);
            let question = match text("question") {
                Some(asked) => candidates
                    .clone()
                    .find(|question| question.text.trim() == asked.trim())
                    .or_else(|| candidates.last()),
                None => candidates.last(),
            }?;
            let key = question
                .options
                .iter()
                .find(|(option, _)| option.trim().eq_ignore_ascii_case(label.trim()))
                .map(|(_, key)| key.clone());
            Some(steer::RemoteAnswer {
                question_id: question.id.clone(),
                ask_id: question.ask_id.clone(),
                // No matching option = a typed reply.
                text: key.is_none().then(|| label.to_string()),
                keys: key.into_iter().collect(),
            })
        }
        "plan" => {
            let question = open.iter().filter(|question| question.plan).last()?;
            let reject = text("decision")? == "rejected";
            let key = question
                .options
                .iter()
                .find(|(_, key)| is_reject(key) == reject)
                .map(|(_, key)| key.clone())?;
            Some(steer::RemoteAnswer {
                question_id: question.id.clone(),
                ask_id: None,
                keys: vec![key],
                text: text("feedback").filter(|_| reject).map(str::to_string),
            })
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn question(id: &str, text: &str, plan: bool, options: &[(&str, &str)]) -> OpenQuestion {
        OpenQuestion {
            id: id.into(),
            ask_id: None,
            text: text.into(),
            plan,
            options: options.iter().map(|(l, k)| (l.to_string(), k.to_string())).collect(),
        }
    }

    #[test]
    fn a_question_answer_maps_its_label_onto_the_engine_key() {
        let open = vec![
            question("q-1", "Which db?", false, &[("Postgres", "1"), ("SQLite", "2")]),
            question("p-1", "The plan", true, &[("Approve", "exit-plan-bypass"), ("No", "reject")]),
        ];
        let card = serde_json::json!({"kind": "question", "question": "Which db?", "answer": "SQLite"});
        let answer = engine_answer(&card, &open).expect("maps");
        assert_eq!(answer.question_id, "q-1");
        assert_eq!(answer.keys, vec!["2".to_string()]);
        assert_eq!(answer.text, None);
    }

    #[test]
    fn a_plan_decision_picks_approve_or_reject() {
        let open = vec![question(
            "p-1",
            "The plan",
            true,
            &[("Approve", "exit-plan-bypass"), ("Keep planning", "reject")],
        )];
        let approved = serde_json::json!({"kind": "plan", "plan": "x", "decision": "approved"});
        let answer = engine_answer(&approved, &open).expect("maps");
        assert_eq!((answer.question_id.as_str(), answer.keys), ("p-1", vec!["exit-plan-bypass".to_string()]));
        let rejected =
            serde_json::json!({"kind": "plan", "plan": "x", "decision": "rejected", "feedback": "smaller"});
        let answer = engine_answer(&rejected, &open).expect("maps");
        assert_eq!(answer.keys, vec!["reject".to_string()]);
        assert_eq!(answer.text.as_deref(), Some("smaller"));
    }

    #[test]
    fn no_open_card_answers_nothing() {
        let card = serde_json::json!({"kind": "question", "question": "Q", "answer": "A"});
        assert!(engine_answer(&card, &[]).is_none());
    }

    #[test]
    fn the_fingerprint_moves_with_the_row_and_its_queue() {
        let row: SessionRow = serde_json::from_value(serde_json::json!({
            "id": "s-1", "status": "queued", "updated_at": "t1",
        }))
        .unwrap();
        let before = fingerprint(&row, &[]);
        let message: MessageRow = serde_json::from_value(serde_json::json!({
            "id": "m-1", "session_id": "s-1", "root_id": "s-1", "author_kind": "user",
        }))
        .unwrap();
        assert_ne!(before, fingerprint(&row, &[message]));
        let mut later = row.clone();
        later.updated_at = Some("t2".into());
        assert_ne!(before, fingerprint(&later, &[]));
    }
}
