//! EXP-1292 — the engine's two writers into the Conversation, each ONE
//! off-thread worker so nothing on the ACP dispatch path ever waits on the
//! network:
//!
//! - **replies**: the mapper's [`Reply`] rows (a turn's final message, the
//!   question/plan/subagent/picture cards) → `sessions.reply`, strictly in
//!   order (a failed post is retried with backoff and nothing behind it
//!   overtakes it). A posted question/plan card's `messages.id` is
//!   remembered against its question id — the host's
//!   [`coding::pickup::LiveView::open_cards`] — so an answer given IN the
//!   card finds the engine's open ask again.
//! - **consumes**: the ids of the person messages the engine actually handed
//!   to the agent → `sessions.consume`, batched (whatever is waiting goes in
//!   one call; `api::session_device::consume` chunks at its cap). A transient
//!   failure retries for as long as the run lives (the call is idempotent,
//!   and an unconsumed row keeps reading `queued` and keeps an agent's
//!   triggers deferred); what is still unconsumed when the run ends is
//!   [`Outbox::unconsumed`], which the end sequence parks on the run record
//!   so the next wake consumes it instead of delivering it twice.
//!
//! Both drain on the run's end: [`Outbox::close`] stops the retries' waits,
//! gives each remaining item one more attempt and waits a bounded
//! [`FLUSH_GRACE`] for the queues to empty, so the last turn's reply lands
//! before the row is reported ended — and a server that is down never holds
//! the end sequence hostage.
//!
//! EXP-1458: a reply's body and card are SEALED to the session's tree
//! (`coding::sealing`, the root as the host's synced rows name it) before
//! they post — once, ahead of the retries. #EDE-97: plaintext only into a
//! clear tree; a seal that fails (no key holder, no key yet, no root known)
//! is retried on the same ladder as a failed post (nothing behind it
//! overtakes it), never posted in clear. #EXP-1959: and never dropped
//! silently: it stays queued, retried at the ladder's last wait for as long
//! as the run lives (a key holder may still come up), and when this
//! process's eNet gave up for good (`sealing::holder_off_reason`) the
//! server posts ONE line saying so in the session (`sessions.noKeyHolder`,
//! its own words in clear, once per session and process). The open-card map reads the
//! unsealed card. A sealed question card carries its `askText` (what a
//! listener's agent reads, sealed to the own tree), and a sealed line its
//! `listenerCopies`: the text re-sealed for every listening tree outside
//! the own one (`sessions.listeners` once, then each sealed reply's
//! `listeners`). Every reply names the members its text mentions
//! (`mentionUserIds`, `@email` resolved against the host's synced users,
//! `sealing::note_users`).

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use api::session_device::{ListenerTree, ReplyCard, ReplyExtras, ReplyResult, SubagentRow};
use api::ApiError;
use serde_json::Value;
use coding::sealing;

use crate::mapper::Reply;

/// How long the end sequence waits for the queues to drain.
pub const FLUSH_GRACE: Duration = Duration::from_secs(5);

/// The retry ladder for a transient failure (offline, a 5xx, 408/429): the
/// waits between attempts, after which a reply is dropped (logged); a
/// consume keeps retrying at the last wait until the run ends.
pub const RETRY_BACKOFF: [Duration; 7] = [
    Duration::from_secs(1),
    Duration::from_secs(2),
    Duration::from_secs(4),
    Duration::from_secs(8),
    Duration::from_secs(15),
    Duration::from_secs(30),
    Duration::from_secs(60),
];

/// How often a waiting worker re-checks its queue and the close flag.
const POLL: Duration = Duration::from_millis(50);

/// The two device procedures the outbox calls — the real tRPC client, or a
/// test's recording fake.
/// EXP-1458: `reply` posts the card as JSON (a sealed card is no
/// [`ReplyCard`]), the body as given (a body envelope when sealed) and the
/// sealed reply's extras; `listeners` = `sessions.listeners`.
pub trait ConversationApi: Send + Sync + 'static {
    fn reply(
        &self,
        session_id: &str,
        body: &str,
        card: Option<&Value>,
        extras: &ReplyExtras,
    ) -> Result<ReplyResult, ApiError>;
    fn listeners(&self, _session_id: &str) -> Result<Vec<ListenerTree>, ApiError> {
        Ok(Vec::new())
    }
    fn consume(&self, session_id: &str, message_ids: &[String]) -> Result<(), ApiError>;
    /// #EXP-1959: `sessions.noKeyHolder`, the session's one line that this
    /// device holds no key holder (`reason` = a `sealing::HolderOff` code).
    fn no_key_holder(&self, _session_id: &str, _reason: &str) -> Result<(), ApiError> {
        Ok(())
    }
    /// #EXP-1731: `sessions.subagent`, one CLI subagent edge under
    /// `session_id` (a fake that does not care records nothing).
    fn subagent(&self, _session_id: &str, _row: &SubagentRow) -> Result<(), ApiError> {
        Ok(())
    }
}

impl ConversationApi for api::TrpcClient {
    fn reply(
        &self,
        session_id: &str,
        body: &str,
        card: Option<&Value>,
        extras: &ReplyExtras,
    ) -> Result<ReplyResult, ApiError> {
        api::session_device::reply_sealed(self, session_id, body, card, extras)
    }

    fn listeners(&self, session_id: &str) -> Result<Vec<ListenerTree>, ApiError> {
        api::session_device::listeners(self, session_id)
    }

    fn consume(&self, session_id: &str, message_ids: &[String]) -> Result<(), ApiError> {
        api::session_device::consume(self, session_id, message_ids)
    }

    fn subagent(&self, session_id: &str, row: &SubagentRow) -> Result<(), ApiError> {
        api::session_device::subagent(self, session_id, row).map(|_| ())
    }

    fn no_key_holder(&self, session_id: &str, reason: &str) -> Result<(), ApiError> {
        api::session_device::no_key_holder(self, session_id, reason)
    }
}

/// A failure worth another attempt: the network, a server error, a timeout
/// or a rate limit. Every other refusal (a 4xx, a decode error) would fail
/// the same way again.
pub fn retryable(err: &ApiError) -> bool {
    match err {
        ApiError::Transport { .. } => true,
        ApiError::Http { status, .. } => *status >= 500 || *status == 408 || *status == 429,
        _ => false,
    }
}

/// The card's question id, for the cards answered IN the Conversation.
fn card_question_id(card: Option<&ReplyCard>) -> Option<&str> {
    match card? {
        ReplyCard::Question { question_id, .. } => Some(question_id),
        _ => None,
    }
}

/// The sealed form of a reply: body, card as posted, the card's plain JSON.
type SealedReply = (String, Option<serde_json::Value>, Option<serde_json::Value>);

/// EXP-1458: what a reply posts as: its body and card sealed to the
/// session's tree (#EDE-97: plaintext only into a clear tree, an `Err`
/// otherwise), plus the card's unsealed JSON. BLOCKING (a first seal may
/// create the tree key or wait for a coming key holder).
fn sealed_reply(session_id: &str, reply: &Reply) -> Result<SealedReply, sealing::SealError> {
    let plain_card = reply.card.as_ref().and_then(|card| match serde_json::to_value(card) {
        Ok(value) => Some(value),
        Err(err) => {
            log::warn!("engine: reply card of {session_id} not serializable: {err}");
            None
        }
    });
    let body = sealing::seal_body_for_session(session_id, &reply.body)?;
    let card = plain_card.as_ref().map(|card| sealing::seal_card_for_session(session_id, card)).transpose()?;
    Ok((body, card, plain_card))
}

/// #EDE-97: [`sealed_reply`] on the retry ladder (a key holder still
/// registering, a key its device's claim is about to create), never sent
/// in clear. #EXP-1959: past the ladder it keeps retrying at its last wait
/// until the run ends (a holder may still come up), and once this process
/// has no key holder for good the session hears it ONCE
/// ([`say_no_key_holder`]). `None` = the run ended (or the reply is
/// invalid) before it could be sealed: dropped, logged.
fn sealed_reply_retrying(
    api: &dyn ConversationApi,
    closing: &AtomicBool,
    session_id: &str,
    reply: &Reply,
) -> Option<SealedReply> {
    let mut waits = RETRY_BACKOFF.iter();
    let last = RETRY_BACKOFF[RETRY_BACKOFF.len() - 1];
    loop {
        match sealed_reply(session_id, reply) {
            Ok(sealed) => return Some(sealed),
            Err(err) => {
                if matches!(err, sealing::SealError::Invalid(_)) || closing.load(Ordering::SeqCst) {
                    log::warn!("engine: a reply of {session_id} was not posted: it could not be encrypted ({err})");
                    return None;
                }
                if let Some(off) = sealing::holder_off_reason() {
                    say_no_key_holder(api, closing, session_id, off);
                }
                let wait = waits.next().unwrap_or(&last);
                log::debug!("engine: a reply of {session_id} could not be sealed yet, retrying in {wait:?}: {err}");
                sleep_unless_closing(*wait, closing);
            }
        }
    }
}

/// #EXP-1959: the sessions this process already told it holds no key
/// holder: ONE line per session, however many replies or runs wait.
static SAID_NO_KEY_HOLDER: Mutex<Option<HashSet<String>>> = Mutex::new(None);

/// #EXP-1959: tells `session_id` (once per process) that its agent's
/// replies cannot be sealed here: `sessions.noKeyHolder`, whose line the
/// server writes in clear (a keyed tree takes no plaintext from a device).
fn say_no_key_holder(api: &dyn ConversationApi, closing: &AtomicBool, session_id: &str, off: sealing::HolderOff) {
    let first = SAID_NO_KEY_HOLDER
        .lock()
        .map(|mut said| said.get_or_insert_with(HashSet::new).insert(session_id.to_string()))
        .unwrap_or(false);
    if !first {
        return;
    }
    log::warn!(
        "engine: replies of {session_id} wait: this device holds no encryption key (P2P is off: {})",
        off.detail()
    );
    let _ = with_retry(closing, "sessions.noKeyHolder", false, || api.no_key_holder(session_id, off.code()));
}

/// EXP-1458: a sealed reply's extras: a question/plan card's ask text
/// sealed to the own tree, and the line a listener reads (the reply text,
/// or the ask text) re-sealed for every keyed listening tree. BLOCKING.
fn reply_extras(
    api: &dyn ConversationApi,
    session_id: &str,
    reply: &Reply,
    plain_card: Option<&Value>,
    listeners: &mut Option<Vec<ListenerTree>>,
) -> ReplyExtras {
    let asks = plain_card.filter(|card| matches!(card.get("kind").and_then(Value::as_str), Some("question" | "plan")));
    let ask = asks.and_then(|card| ask_text(session_id, card));
    let ask_text = ask.as_deref().and_then(|text| {
        let sealed = sealing::seal_body_for_session(session_id, text).ok()?;
        sealed.starts_with(sealing::SEALED_BODY_PREFIX).then_some(sealed)
    });
    // What reaches a listener: the ask, or a turn's text (a subagent /
    // picture card relays nothing).
    let line = match (&ask, plain_card) {
        (Some(ask), _) => Some(ask.as_str()),
        (None, None) if !reply.body.trim().is_empty() => Some(reply.body.as_str()),
        _ => None,
    };
    let copies = line.map(|line| {
        if listeners.is_none() {
            match api.listeners(session_id) {
                Ok(trees) => *listeners = Some(trees),
                Err(err) => log::warn!("engine: sessions.listeners of {session_id} failed: {err}"),
            }
        }
        listeners.as_deref().unwrap_or_default().iter().filter_map(|tree| sealing::listener_copy(tree, line)).collect::<Vec<_>>()
    });
    ReplyExtras {
        ask_text,
        listener_copies: copies.filter(|copies| !copies.is_empty()),
        ..ReplyExtras::default()
    }
}

/// The members a reply's text and its card's ask mention (`@email`,
/// resolved against the host's synced users): a sealed body's are
/// unreadable to the server, so every reply names them.
fn reply_mentions(session_id: &str, reply: &Reply, plain_card: Option<&Value>) -> Option<Vec<String>> {
    let asks = plain_card.filter(|card| matches!(card.get("kind").and_then(Value::as_str), Some("question" | "plan")));
    let ask = asks.and_then(|card| ask_text(session_id, card));
    let mut ids: Vec<String> = Vec::new();
    for id in [Some(reply.body.as_str()), ask.as_deref()].into_iter().flatten().flat_map(sealing::mention_user_ids) {
        if !ids.contains(&id) {
            ids.push(id);
        }
    }
    (!ids.is_empty()).then_some(ids)
}

/// How long a plan's text runs in an ask before it is clipped.
const ASK_PLAN_MAX: usize = 2000;

/// EXP-1458: the text a listener's agent reads for an open card — the
/// server's `askText` (apps/web lib/sessions/bus.ts), which it can no
/// longer compose from a sealed card.
pub(crate) fn ask_text(session_id: &str, card: &Value) -> Option<String> {
    let copy = &domain::chats::CHATS.copy.preview;
    let send = format!("exponential_sessions_send({{sessionId: \"{session_id}\", answer: {{…}}}})");
    let str_of = |value: &Value, key: &str| value.get(key).and_then(Value::as_str).unwrap_or_default().to_string();
    match card.get("kind").and_then(Value::as_str)? {
        "question" => {
            let options: Vec<&str> =
                card.get("options").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str).collect();
            let mut flags = Vec::new();
            if card.get("multi").and_then(Value::as_bool) == Some(true) {
                flags.push("several allowed");
            }
            if card.get("custom").and_then(Value::as_bool) != Some(false) {
                flags.push("free text allowed");
            }
            let mut lines = vec![format!("{}{}", copy.ask_question_lead, str_of(card, "question"))];
            if !options.is_empty() {
                let flags = if flags.is_empty() { String::new() } else { format!(" ({})", flags.join(", ")) };
                lines.push(format!("Options: {}{flags}", options.join(" | ")));
            }
            lines.push(format!(
                "Answer with {send} where answer = {{answers: [\"<option>\"]}} or {{answer: \"<text>\"}} if you know it; otherwise end your turn, a person answers in the card."
            ));
            Some(lines.join("\n"))
        }
        "plan" => {
            let plan = str_of(card, "plan");
            let plan = if plan.chars().count() > ASK_PLAN_MAX {
                format!("{}…", plan.chars().take(ASK_PLAN_MAX - 1).collect::<String>().trim_end())
            } else {
                plan
            };
            let questions: Vec<String> = card
                .get("questions")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .map(|question| {
                    let labels: Vec<String> = question
                        .get("options")
                        .and_then(Value::as_array)
                        .into_iter()
                        .flatten()
                        .map(|option| str_of(option, "label"))
                        .collect();
                    format!("- {}: {} ({})", str_of(question, "id"), str_of(question, "prompt"), labels.join(" | "))
                })
                .collect();
            let mut blocks = vec![copy.ask_plan_lead.clone(), plan];
            if !questions.is_empty() {
                blocks.push(format!("Questions:\n{}", questions.join("\n")));
            }
            blocks.push(format!(
                "Decide with {send} where answer = {{decision: \"approved\" | \"rejected\", answers: {{\"<question id>\": [\"<option>\"]}}, feedback: \"<text>\"}} if it is yours to decide; otherwise end your turn, a person decides in the card."
            ));
            Some(blocks.join("\n\n"))
        }
        _ => None,
    }
}

/// What the reply worker posts, in queue order: a Conversation row, or
/// (#EXP-1731) a CLI subagent's child-row edge — one queue, so an `ended`
/// never overtakes its `running`, nor the subagent card beside it.
enum Outgoing {
    Reply(Reply),
    Subagent(SubagentRow),
}

/// One run's outbox. Cheap to share; the workers own clones of the state.
pub struct Outbox {
    replies: flume::Sender<Outgoing>,
    consumes: flume::Sender<String>,
    shared: Arc<Shared>,
    /// #PERF-12: wakes the idle workers for [`Outbox::close`] (they wait on
    /// the runtime, not in a `recv_timeout` loop).
    closing: tokio::sync::watch::Sender<bool>,
    done: Vec<flume::Receiver<()>>,
}

#[derive(Default)]
struct Shared {
    closing: AtomicBool,
    /// Posted, still-open question/plan cards: `messages.id` → question id.
    open_cards: Mutex<HashMap<String, String>>,
    /// Questions resolved on the engine side before (or after) their card
    /// posted — a card resolved first never becomes "open".
    resolved: Mutex<HashSet<String>>,
    /// Consume ids queued and not settled yet (consumed, or refused as
    /// unknown) — [`Outbox::unconsumed`].
    unconsumed: Mutex<HashSet<String>>,
}

impl Outbox {
    /// Spawn both workers for `session_id`, as tasks on `runtime`
    /// (#PERF-12: two threads per run before). A worker waits for its queue
    /// on the runtime and posts on its blocking pool — the posts are
    /// blocking HTTP, a seal, the retry ladder's waits — giving the pool
    /// thread back once the queue is empty.
    pub fn spawn(runtime: &tokio::runtime::Handle, api: Arc<dyn ConversationApi>, session_id: &str) -> Outbox {
        let shared = Arc::new(Shared::default());
        let (replies, reply_rx) = flume::unbounded::<Outgoing>();
        let (consumes, consume_rx) = flume::unbounded::<String>();
        let (closing, closing_rx) = tokio::sync::watch::channel(false);
        let mut done = Vec::new();

        let (reply_done, reply_done_rx) = flume::bounded(1);
        done.push(reply_done_rx);
        {
            let api = Arc::clone(&api);
            let shared = Arc::clone(&shared);
            let session_id = session_id.to_string();
            // EXP-1458: the listening trees, fetched before the first
            // sealed line that needs copies, then kept current off each
            // sealed reply's result.
            let mut listeners: Option<Vec<ListenerTree>> = None;
            spawn_worker(runtime, reply_rx, closing_rx.clone(), reply_done, move |item, _queue| {
                let reply = match item {
                    Outgoing::Reply(reply) => reply,
                    Outgoing::Subagent(mut row) => {
                        // #FEED-192: a keyed tree takes no plaintext from the device: its
                        // subagent rows go out without the model's title and summary.
                        if sealing::root_of(&session_id).is_some_and(|root| sealing::tree_keyed(&root)) {
                            row.title = None;
                            row.summary = None;
                        }
                        let _ = with_retry(&shared.closing, "sessions.subagent", false, || {
                            api.subagent(&session_id, &row)
                        });
                        return;
                    }
                };
                let Some((body, card, plain_card)) = sealed_reply_retrying(api.as_ref(), &shared.closing, &session_id, &reply) else {
                    return;
                };
                let sealed = body.starts_with(sealing::SEALED_BODY_PREFIX)
                    || card.as_ref().is_some_and(|card| card.get("sealed").is_some());
                let mut extras = if sealed {
                    reply_extras(api.as_ref(), &session_id, &reply, plain_card.as_ref(), &mut listeners)
                } else {
                    ReplyExtras::default()
                };
                extras.mention_user_ids = reply_mentions(&session_id, &reply, plain_card.as_ref());
                let posted = with_retry(&shared.closing, "sessions.reply", false, || {
                    api.reply(&session_id, &body, card.as_ref(), &extras)
                })
                .map(|result| {
                    if sealed {
                        listeners = Some(result.listeners);
                    }
                    result.message_id
                });
                // A sealed question card's full form, for the device's
                // write-back of an answer given in the session view.
                if let (Some(message_id), Some(plain), Some(sealed)) = (&posted, &plain_card, &card) {
                    if sealed.get("sealed").is_some() && card_question_id(reply.card.as_ref()).is_some() {
                        coding::pickup::remember_sealed_card(message_id, &session_id, plain);
                    }
                }
                if let (Some(message_id), Some(question_id)) =
                    (posted, card_question_id(reply.card.as_ref()))
                {
                    let resolved = shared
                        .resolved
                        .lock()
                        .map(|mut resolved| resolved.remove(question_id))
                        .unwrap_or(false);
                    if !resolved {
                        if let Ok(mut open) = shared.open_cards.lock() {
                            open.insert(message_id, question_id.to_string());
                        }
                    }
                }
            });
        }

        let (consume_done, consume_done_rx) = flume::bounded(1);
        done.push(consume_done_rx);
        {
            let shared = Arc::clone(&shared);
            let session_id = session_id.to_string();
            spawn_worker(runtime, consume_rx, closing_rx, consume_done, move |first, queue| {
                let mut ids = vec![first];
                ids.extend(queue.drain());
                ids.dedup();
                let settled = consume_batch(api.as_ref(), &shared.closing, &session_id, ids);
                if let Ok(mut unconsumed) = shared.unconsumed.lock() {
                    for id in &settled {
                        unconsumed.remove(id);
                    }
                }
            });
        }

        Outbox {
            replies,
            consumes,
            shared,
            closing,
            done,
        }
    }

    /// Queue rows for the Conversation, in order.
    pub fn post(&self, replies: Vec<Reply>) {
        for reply in replies {
            let _ = self.replies.send(Outgoing::Reply(reply));
        }
    }

    /// #EXP-1731: queue CLI subagent edges (`sessions.subagent`), in order,
    /// behind whatever replies are already queued.
    pub fn post_subagents(&self, rows: Vec<SubagentRow>) {
        for row in rows {
            let _ = self.replies.send(Outgoing::Subagent(row));
        }
    }

    /// Queue person message ids the agent was handed.
    pub fn consume(&self, ids: impl IntoIterator<Item = String>) {
        for id in ids {
            if let Ok(mut unconsumed) = self.shared.unconsumed.lock() {
                unconsumed.insert(id.clone());
            }
            let _ = self.consumes.send(id);
        }
    }

    /// EXP-1292: the handed ids whose consume has not landed — after
    /// [`Outbox::close`], the ones the run ends owing the server. Sorted.
    pub fn unconsumed(&self) -> Vec<String> {
        let mut ids: Vec<String> = self
            .shared
            .unconsumed
            .lock()
            .map(|ids| ids.iter().cloned().collect())
            .unwrap_or_default();
        ids.sort_unstable();
        ids
    }

    /// The engine resolved `question_id` (answered anywhere, or dismissed):
    /// its card is no longer open.
    pub fn resolve_question(&self, question_id: &str) {
        let removed = self
            .shared
            .open_cards
            .lock()
            .map(|mut open| {
                let before = open.len();
                open.retain(|_, open_question| open_question != question_id);
                open.len() != before
            })
            .unwrap_or(false);
        if !removed {
            if let Ok(mut resolved) = self.shared.resolved.lock() {
                resolved.insert(question_id.to_string());
            }
        }
    }

    /// `messages.id` → question id of every posted card still open.
    pub fn open_cards(&self) -> HashMap<String, String> {
        self.shared
            .open_cards
            .lock()
            .map(|open| open.clone())
            .unwrap_or_default()
    }

    /// The end sequence's flush: no more waiting between retries, then up
    /// to `grace` for both queues to drain. Idempotent.
    pub fn close(&self, grace: Duration) {
        self.shared.closing.store(true, Ordering::SeqCst);
        self.closing.send_replace(true);
        let deadline = Instant::now() + grace;
        for done in &self.done {
            let left = deadline.saturating_duration_since(Instant::now());
            if done.recv_timeout(left).is_err() {
                log::warn!("engine: the Conversation outbox did not drain within {grace:?}");
                return;
            }
        }
    }
}

/// #PERF-12: one outbox worker. It waits for its queue on the runtime and
/// runs `step` on the blocking pool for each item — the first, and every one
/// queued meanwhile — then gives the pool thread back. Ends once the outbox
/// is closing and the queue is empty (or every sender is gone), then says so
/// on `done`. `step` sees the queue too (the consumes batch what is there).
fn spawn_worker<T: Send + 'static>(
    runtime: &tokio::runtime::Handle,
    queue: flume::Receiver<T>,
    mut closing: tokio::sync::watch::Receiver<bool>,
    done: flume::Sender<()>,
    step: impl FnMut(T, &flume::Receiver<T>) + Send + 'static,
) {
    runtime.spawn(async move {
        let mut step = Some(step);
        while let Some(first) = next(&queue, &mut closing).await {
            let Some(mut run) = step.take() else { return };
            let queue = queue.clone();
            let ran = tokio::task::spawn_blocking(move || {
                run(first, &queue);
                while let Ok(item) = queue.try_recv() {
                    run(item, &queue);
                }
                run
            })
            .await;
            match ran {
                Ok(run) => step = Some(run),
                // A panicking post: `done` goes unsent, the way a dead
                // worker thread's did (`close` logs the undrained outbox).
                Err(_) => return,
            }
        }
        let _ = done.send(());
    });
}

/// The next queued item; `None` once the outbox is closing and the queue is
/// empty (or every sender is gone).
async fn next<T>(queue: &flume::Receiver<T>, closing: &mut tokio::sync::watch::Receiver<bool>) -> Option<T> {
    loop {
        tokio::select! {
            biased;
            item = queue.recv_async() => return item.ok(),
            // A gone `Outbox` (its sender dropped) is closing too.
            () = async { let _ = closing.wait_for(|closing| *closing).await; } => {
                if queue.is_empty() {
                    return None;
                }
            }
        }
    }
}

/// One call with the retry ladder. While the outbox is closing a transient
/// failure is not retried (the run is ending; the grace is short).
/// `until_closing`: past the ladder keep retrying at its last wait instead
/// of dropping the item (consumes — idempotent, and owed).
fn with_retry<R>(
    closing: &AtomicBool,
    what: &str,
    until_closing: bool,
    mut call: impl FnMut() -> Result<R, ApiError>,
) -> Option<R> {
    let mut waits = RETRY_BACKOFF.iter();
    let last = RETRY_BACKOFF[RETRY_BACKOFF.len() - 1];
    loop {
        match call() {
            Ok(value) => return Some(value),
            Err(err) if retryable(&err) && !closing.load(Ordering::SeqCst) => {
                let wait = match waits.next() {
                    Some(wait) => wait,
                    None if until_closing => &last,
                    None => {
                        log::warn!("engine: {what} still failing, dropped: {err}");
                        return None;
                    }
                };
                log::debug!("engine: {what} failed, retrying in {wait:?}: {err}");
                sleep_unless_closing(*wait, closing);
            }
            Err(err) => {
                log::warn!("engine: {what} failed: {err}");
                return None;
            }
        }
    }
}

/// A consume batch; returns the SETTLED ids (consumed, or refused as no
/// longer known — nothing left to owe). A refused batch of several (one id
/// the server no longer knows — a message unqueued after it was handed
/// over) is retried id by id, so one stale id never strands the rest.
fn consume_batch(
    api: &dyn ConversationApi,
    closing: &AtomicBool,
    session_id: &str,
    ids: Vec<String>,
) -> Vec<String> {
    let mut refused = false;
    let sent = with_retry(closing, "sessions.consume", true, || {
        let result = api.consume(session_id, &ids);
        if let Err(err) = &result {
            refused = !retryable(err);
        }
        result
    });
    if sent.is_some() || (refused && ids.len() == 1) {
        return ids;
    }
    if !refused {
        // Still failing as the run ends: owed (`Outbox::unconsumed`).
        return Vec::new();
    }
    ids.into_iter()
        .filter(|id| {
            let mut refused = false;
            let sent = with_retry(closing, "sessions.consume", true, || {
                let result = api.consume(session_id, std::slice::from_ref(id));
                if let Err(err) = &result {
                    refused = !retryable(err);
                }
                result
            });
            sent.is_some() || refused
        })
        .collect()
}

fn sleep_unless_closing(wait: Duration, closing: &AtomicBool) {
    let deadline = Instant::now() + wait;
    while Instant::now() < deadline && !closing.load(Ordering::SeqCst) {
        std::thread::sleep(POLL.min(deadline.saturating_duration_since(Instant::now())));
    }
}

/// EXP-1458: a [`sealing::SealService`] over an in-memory keyring for this
/// crate's tests (coding's own fake is test-only there). Installing it is
/// process-global: hold [`fake_seal::serial`], and use session ids no other
/// test feeds to the keyring.
#[cfg(test)]
pub(crate) mod fake_seal {
    use super::sealing::{self, Keyring, SealService};
    use std::sync::{Arc, Mutex};

    static SERIAL: Mutex<()> = Mutex::new(());

    pub(crate) fn serial() -> std::sync::MutexGuard<'static, ()> {
        SERIAL.lock().unwrap_or_else(|p| p.into_inner())
    }

    #[derive(Default)]
    pub(crate) struct Fake {
        pub(crate) keyring: Keyring,
    }

    impl SealService for Fake {
        fn keyring(&self) -> &Keyring {
            &self.keyring
        }
        fn install_id(&self) -> Option<String> {
            Some("install-1".into())
        }
        fn ensure_key(&self, root_id: &str) -> Result<(u32, [u8; 32]), sealing::SealError> {
            if let Some(current) = self.keyring.current(root_id) {
                return Ok(current);
            }
            let key = sealing::generate_tree_key();
            self.keyring.insert_secret(root_id, 1, key.secret);
            self.keyring.set_current(root_id, 1, key.public);
            Ok((1, key.public))
        }
        fn public_key(&self, root_id: &str) -> Result<Option<(u32, [u8; 32])>, String> {
            Ok(self.keyring.current(root_id))
        }
        fn fetch_secrets(&self, _root_id: &str) {}
    }

    pub(crate) fn install() -> Arc<Fake> {
        let fake = Arc::new(Fake::default());
        sealing::install(fake.clone());
        fake
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The workers' runtime (a host's shared one in production).
    fn test_runtime() -> &'static tokio::runtime::Handle {
        static RUNTIME: std::sync::OnceLock<tokio::runtime::Runtime> = std::sync::OnceLock::new();
        RUNTIME
            .get_or_init(|| tokio::runtime::Builder::new_multi_thread().worker_threads(1).enable_all().build().unwrap())
            .handle()
    }

    #[derive(Debug, Clone, PartialEq)]
    enum Call {
        Reply(String, Option<serde_json::Value>),
        Consume(Vec<String>),
        /// #EXP-1731: `sessions.subagent` — the key and the status.
        Subagent(String, String),
    }

    /// Records every call; the first `fail_replies` replies fail with a
    /// transient error, and any consume naming `stale` is refused (400).
    #[derive(Default)]
    struct Fake {
        calls: Mutex<Vec<Call>>,
        fail_replies: Mutex<usize>,
        stale: Option<String>,
        ids: std::sync::atomic::AtomicUsize,
        extras: Mutex<Vec<ReplyExtras>>,
        trees: Mutex<Vec<ListenerTree>>,
        listener_calls: Mutex<usize>,
        /// #EXP-1959: `sessions.noKeyHolder` calls, `(session, reason)`.
        no_key_holder: Mutex<Vec<(String, String)>>,
    }

    impl ConversationApi for Fake {
        fn reply(
            &self,
            _session: &str,
            body: &str,
            card: Option<&serde_json::Value>,
            extras: &ReplyExtras,
        ) -> Result<ReplyResult, ApiError> {
            self.extras.lock().unwrap().push(extras.clone());
            {
                let mut fail = self.fail_replies.lock().unwrap();
                if *fail > 0 {
                    *fail -= 1;
                    return Err(ApiError::Http { status: 503, message: "down".into() });
                }
            }
            self.calls.lock().unwrap().push(Call::Reply(body.to_string(), card.cloned()));
            let message_id = format!("msg-{}", self.ids.fetch_add(1, Ordering::SeqCst) + 1);
            Ok(ReplyResult { message_id, listeners: self.trees.lock().unwrap().clone() })
        }

        fn listeners(&self, _session: &str) -> Result<Vec<ListenerTree>, ApiError> {
            *self.listener_calls.lock().unwrap() += 1;
            Ok(self.trees.lock().unwrap().clone())
        }

        fn consume(&self, _session: &str, ids: &[String]) -> Result<(), ApiError> {
            if self.stale.as_ref().is_some_and(|stale| ids.contains(stale)) {
                return Err(ApiError::Http { status: 400, message: "unknown message".into() });
            }
            self.calls.lock().unwrap().push(Call::Consume(ids.to_vec()));
            Ok(())
        }

        fn no_key_holder(&self, session: &str, reason: &str) -> Result<(), ApiError> {
            self.no_key_holder.lock().unwrap().push((session.to_string(), reason.to_string()));
            Ok(())
        }

        fn subagent(&self, _session: &str, row: &SubagentRow) -> Result<(), ApiError> {
            self.calls
                .lock()
                .unwrap()
                .push(Call::Subagent(row.key.clone(), row.status.as_str().to_string()));
            Ok(())
        }
    }

    /// #EDE-97: a session of a clear tree (plaintext is its legal wire):
    /// the tests about the queue, not about sealing, write into one.
    fn clear(id: &'static str) -> &'static str {
        sealing::note_clear_tree(id);
        id
    }

    fn question(id: &str) -> ReplyCard {
        ReplyCard::Question {
            question: "Which?".into(),
            options: vec!["A".into(), "B".into()],
            question_id: id.into(),
        }
    }

    #[test]
    fn replies_post_in_order_and_cards_stay_open_until_resolved() {
        let fake = Arc::new(Fake::default());
        let outbox = Outbox::spawn(test_runtime(), fake.clone(), clear("sess-1"));
        outbox.post(vec![
            Reply { body: "first".into(), card: None },
            Reply { body: String::new(), card: Some(question("q-1")) },
            Reply { body: "last".into(), card: None },
        ]);
        outbox.close(Duration::from_secs(5));
        assert_eq!(
            *fake.calls.lock().unwrap(),
            vec![
                Call::Reply("first".into(), None),
                Call::Reply(String::new(), Some(serde_json::to_value(question("q-1")).unwrap())),
                Call::Reply("last".into(), None),
            ]
        );
        assert_eq!(
            outbox.open_cards(),
            HashMap::from([("msg-2".to_string(), "q-1".to_string())])
        );
        outbox.resolve_question("q-1");
        assert!(outbox.open_cards().is_empty());
    }

    /// #EXP-1731: a subagent's edges ride the reply queue — an `ended`
    /// never overtakes its `running`, nor the card queued before it.
    #[test]
    fn subagent_edges_post_in_order_with_the_replies() {
        let fake = Arc::new(Fake::default());
        let outbox = Outbox::spawn(test_runtime(), fake.clone(), clear("sess-1"));
        let row = |status| SubagentRow {
            key: "task-1".into(),
            status,
            title: Some("Find the callers".into()),
            summary: None,
            failed: false,
        };
        outbox.post_subagents(vec![row(api::session_device::SubagentRowStatus::Running)]);
        outbox.post(vec![Reply { body: "mid".into(), card: None }]);
        outbox.post_subagents(vec![row(api::session_device::SubagentRowStatus::Ended)]);
        outbox.close(Duration::from_secs(5));
        assert_eq!(
            *fake.calls.lock().unwrap(),
            vec![
                Call::Subagent("task-1".into(), "running".into()),
                Call::Reply("mid".into(), None),
                Call::Subagent("task-1".into(), "ended".into()),
            ]
        );
    }

    /// A transient failure is retried and nothing behind it overtakes it.
    #[test]
    fn a_failed_reply_is_retried_before_the_next_one_posts() {
        let fake = Arc::new(Fake::default());
        *fake.fail_replies.lock().unwrap() = 1;
        let outbox = Outbox::spawn(test_runtime(), fake.clone(), clear("sess-1"));
        outbox.post(vec![
            Reply { body: "one".into(), card: None },
            Reply { body: "two".into(), card: None },
        ]);
        let deadline = Instant::now() + Duration::from_secs(10);
        while fake.calls.lock().unwrap().len() < 2 && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
        }
        assert_eq!(
            *fake.calls.lock().unwrap(),
            vec![Call::Reply("one".into(), None), Call::Reply("two".into(), None)]
        );
        outbox.close(Duration::from_secs(1));
    }

    /// A card the engine resolved before its post landed never reads open.
    #[test]
    fn a_card_resolved_before_it_posted_is_not_open() {
        let fake = Arc::new(Fake::default());
        let outbox = Outbox::spawn(test_runtime(), fake.clone(), clear("sess-1"));
        outbox.resolve_question("q-9");
        outbox.post(vec![Reply { body: String::new(), card: Some(question("q-9")) }]);
        outbox.close(Duration::from_secs(5));
        assert!(outbox.open_cards().is_empty());
    }

    #[test]
    fn consumed_ids_batch_and_a_stale_id_does_not_strand_the_rest() {
        let fake = Arc::new(Fake {
            stale: Some("m-gone".into()),
            ..Fake::default()
        });
        let outbox = Outbox::spawn(test_runtime(), fake.clone(), clear("sess-1"));
        outbox.consume(["m-1".to_string(), "m-gone".to_string(), "m-2".to_string()]);
        outbox.close(Duration::from_secs(5));
        let consumed: Vec<String> = fake
            .calls
            .lock()
            .unwrap()
            .iter()
            .flat_map(|call| match call {
                Call::Consume(ids) => ids.clone(),
                _ => Vec::new(),
            })
            .collect();
        assert_eq!(consumed, vec!["m-1".to_string(), "m-2".to_string()]);
    }

    /// A consume still failing when the run ends stays owed; consumed and
    /// stale ids are settled.
    #[test]
    fn a_consume_the_server_never_took_is_reported_unconsumed() {
        struct Down;
        impl ConversationApi for Down {
            fn reply(&self, _: &str, _: &str, _: Option<&serde_json::Value>, _: &ReplyExtras) -> Result<ReplyResult, ApiError> {
                Ok(ReplyResult { message_id: "m".into(), listeners: Vec::new() })
            }
            fn consume(&self, _: &str, _: &[String]) -> Result<(), ApiError> {
                Err(ApiError::transport("offline"))
            }
        }
        let outbox = Outbox::spawn(test_runtime(), Arc::new(Down), clear("sess-1"));
        outbox.consume(["m-2".to_string(), "m-1".to_string()]);
        outbox.close(Duration::from_secs(5));
        assert_eq!(outbox.unconsumed(), vec!["m-1".to_string(), "m-2".to_string()]);

        let fake = Arc::new(Fake { stale: Some("m-gone".into()), ..Fake::default() });
        let outbox = Outbox::spawn(test_runtime(), fake, clear("sess-1"));
        outbox.consume(["m-1".to_string(), "m-gone".to_string()]);
        outbox.close(Duration::from_secs(5));
        assert!(outbox.unconsumed().is_empty());
    }

    /// EXP-1458: with a key holder and the session's root known, the body
    /// and the card post sealed (no plaintext on the wire), the open-card
    /// map still keys the posted card by its question id, and the full card
    /// is kept for the device's write-back.
    #[test]
    fn replies_post_sealed_and_cards_still_open_by_question_id() {
        let _serial = fake_seal::serial();
        let fake = fake_seal::install();
        fake.keyring.note_root("sealed-child", "sealed-root");
        let api = Arc::new(Fake::default());
        // One keyed listening tree (its secret held here only to check the
        // copy), one keyless.
        let listener = sealing::generate_tree_key();
        fake.keyring.insert_secret("listener-root", 4, listener.secret);
        *api.trees.lock().unwrap() = vec![
            ListenerTree {
                root_id: "listener-root".into(),
                epoch: Some(4),
                public_key: Some(sealing::b64url_encode(&listener.public)),
            },
            ListenerTree { root_id: "keyless-root".into(), epoch: None, public_key: None },
        ];
        let outbox = Outbox::spawn(test_runtime(), api.clone(), "sealed-child");
        outbox.post(vec![
            Reply { body: "the secret answer".into(), card: None },
            Reply { body: String::new(), card: Some(question("q-7")) },
        ]);
        outbox.close(Duration::from_secs(5));
        let calls = api.calls.lock().unwrap().clone();
        let [Call::Reply(body, None), Call::Reply(empty, Some(card))] = calls.as_slice() else {
            sealing::uninstall();
            panic!("{calls:?}");
        };
        // The tree's secret opens both back.
        let opened_body = sealing::open_body("sealed-root", body).text;
        let opened_card = sealing::open_card("sealed-root", card).card;
        let extras = api.extras.lock().unwrap().clone();
        let copy_of = |extras: &ReplyExtras| -> Vec<(String, Option<String>)> {
            extras
                .listener_copies
                .iter()
                .flatten()
                .map(|copy| (copy.root_id.clone(), sealing::open_body("listener-root", &copy.body).text))
                .collect()
        };
        let text_copies = copy_of(&extras[0]);
        let ask_copies = copy_of(&extras[1]);
        let ask = extras[1].ask_text.as_deref().and_then(|t| sealing::open_body("sealed-root", t).text);
        sealing::uninstall();
        assert_eq!(*api.listener_calls.lock().unwrap(), 1, "fetched once, then kept off the results");
        assert_eq!(extras[0].ask_text, None);
        assert_eq!(text_copies, vec![("listener-root".to_string(), Some("the secret answer".to_string()))]);
        let ask = ask.expect("a sealed ask text");
        assert!(ask.starts_with("Asks: Which?\nOptions: A | B (free text allowed)\nAnswer with exponential_sessions_send({sessionId: \"sealed-child\""), "{ask}");
        assert_eq!(ask_copies, vec![("listener-root".to_string(), Some(ask.clone()))]);
        assert!(body.starts_with("exp-sealed:v1:1:"), "{body}");
        assert!(!body.contains("secret"));
        assert_eq!(empty, "");
        assert_eq!(card["kind"], "question");
        assert!(card["sealed"].as_str().is_some_and(|s| s.starts_with("v1:1:")), "{card}");
        assert!(card.get("question").is_none() && card.get("questionId").is_none() && card.get("options").is_none());
        assert_eq!(outbox.open_cards(), HashMap::from([("msg-2".to_string(), "q-7".to_string())]));
        assert_eq!(opened_body.as_deref(), Some("the secret answer"));
        assert_eq!(opened_card["questionId"], "q-7");
        assert_eq!(opened_card["question"], "Which?");
    }

    /// EXP-1458: a reply names the members its text and its ask mention,
    /// resolved against the host's synced users; sealed or not.
    #[test]
    fn replies_name_the_members_they_mention() {
        let _serial = fake_seal::serial();
        sealing::note_users([
            ("u-ada".to_string(), "ada@team.test".to_string()),
            ("u-bob".to_string(), "Bob@Team.test".to_string()),
        ]);
        let api = Arc::new(Fake::default());
        let outbox = Outbox::spawn(test_runtime(), api.clone(), clear("plain-child"));
        outbox.post(vec![
            Reply { body: "ping @bob@team.test and @ada@team.test (again @ada@team.test), not @eve@team.test".into(), card: None },
            Reply { body: "no one".into(), card: None },
        ]);
        outbox.close(Duration::from_secs(5));
        sealing::note_users([]);
        let extras = api.extras.lock().unwrap().clone();
        assert_eq!(extras[0].mention_user_ids, Some(vec!["u-bob".to_string(), "u-ada".to_string()]));
        assert_eq!(extras[1].mention_user_ids, None);
    }

    #[test]
    fn only_transient_failures_retry() {
        assert!(retryable(&ApiError::transport("offline")));
        assert!(retryable(&ApiError::Http { status: 502, message: String::new() }));
        assert!(retryable(&ApiError::Http { status: 429, message: String::new() }));
        assert!(!retryable(&ApiError::Http { status: 400, message: String::new() }));
        assert!(!retryable(&ApiError::Unauthorized));
    }

    /// #EDE-97: no silent plaintext. A reply that cannot be sealed yet (no
    /// key holder) is held and retried, so it posts SEALED once a holder
    /// runs; one still unsealable when the run ends is dropped, never
    /// posted in clear.
    #[test]
    fn an_unsealable_reply_waits_for_a_holder_and_never_posts_in_clear() {
        let _serial = fake_seal::serial();
        sealing::uninstall();
        let api = Arc::new(Fake::default());
        let outbox = Outbox::spawn(test_runtime(), api.clone(), "late-child");
        outbox.post(vec![Reply { body: "the late secret".into(), card: None }]);
        std::thread::sleep(Duration::from_millis(300));
        assert!(api.calls.lock().unwrap().is_empty(), "nothing leaves without a key holder");
        let fake = fake_seal::install();
        fake.keyring.note_root("late-child", "late-root");
        let deadline = Instant::now() + Duration::from_secs(10);
        while api.calls.lock().unwrap().is_empty() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
        }
        sealing::uninstall();
        outbox.post(vec![Reply { body: "never in clear".into(), card: None }]);
        outbox.close(Duration::from_secs(5));
        let calls = api.calls.lock().unwrap().clone();
        let [Call::Reply(body, None)] = calls.as_slice() else { panic!("{calls:?}") };
        assert!(body.starts_with(sealing::SEALED_BODY_PREFIX), "{body}");
        assert!(!calls.iter().any(|call| matches!(call, Call::Reply(body, _) if body.contains("clear"))));
    }

    /// #EXP-1959: a KEYED existing tree (its root and child as the synced
    /// `sessions` shape sends them) in a process whose eNet node failed to
    /// start: the replies are neither dropped nor posted in clear. They stay
    /// queued, the session hears ONE `sessions.noKeyHolder` line naming why,
    /// and they post sealed, in order, once a key holder comes up after all.
    #[test]
    fn without_a_key_holder_replies_wait_and_the_session_hears_it_once() {
        let _serial = fake_seal::serial();
        sealing::uninstall();
        let tree = sealing::generate_tree_key();
        let public = sealing::b64url_encode(&tree.public);
        let rows: Vec<domain::rows::SessionRow> = serde_json::from_value(serde_json::json!([
            {"id": "nkh-root", "root_id": "nkh-root", "seal_public_key": public, "seal_epoch": 1, "clear_tree": false},
            {"id": "nkh-child", "parent_id": "nkh-root", "root_id": "nkh-root"},
        ]))
        .unwrap();
        sealing::note_sessions(&rows);
        assert!(sealing::tree_keyed("nkh-root"));
        // What `steer::enet` does when `Node::start` fails.
        sealing::holder_off(sealing::HolderOff::NodeFailed);
        assert_eq!(sealing::holder_off_reason(), Some(sealing::HolderOff::NodeFailed));

        let api = Arc::new(Fake::default());
        let outbox = Outbox::spawn(test_runtime(), api.clone(), "nkh-child");
        outbox.post(vec![
            Reply { body: "the first answer".into(), card: None },
            Reply { body: "the second answer".into(), card: None },
        ]);
        let deadline = Instant::now() + Duration::from_secs(5);
        while api.no_key_holder.lock().unwrap().is_empty() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
        }
        // Past the first rungs of the ladder: still held, said once.
        std::thread::sleep(Duration::from_millis(3500));
        assert!(api.calls.lock().unwrap().is_empty(), "nothing in clear");
        assert_eq!(
            api.no_key_holder.lock().unwrap().clone(),
            vec![("nkh-child".to_string(), "node_failed".to_string())]
        );

        // Signed in again: a holder that reads the tree comes up.
        sealing::expect_service(true);
        assert_eq!(sealing::holder_off_reason(), None);
        let fake = fake_seal::install();
        fake.keyring.note_sessions(&rows);
        fake.keyring.insert_secret("nkh-root", 1, tree.secret);
        let deadline = Instant::now() + Duration::from_secs(20);
        while api.calls.lock().unwrap().len() < 2 && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
        }
        outbox.close(Duration::from_secs(5));
        let calls = api.calls.lock().unwrap().clone();
        let opened: Vec<String> = calls
            .iter()
            .map(|call| {
                let Call::Reply(body, None) = call else { panic!("{calls:?}") };
                assert!(body.starts_with(sealing::SEALED_BODY_PREFIX), "{body}");
                sealing::open_body_text("nkh-root", body).unwrap()
            })
            .collect();
        sealing::uninstall();
        sealing::expect_service(false);
        assert_eq!(opened, ["the first answer", "the second answer"]);
        assert_eq!(api.no_key_holder.lock().unwrap().len(), 1, "one line per session");
    }
}
