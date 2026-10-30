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
//! they post — once, ahead of the retries; with no key holder or no root
//! known they go out plaintext (still legal). The open-card map reads the
//! unsealed card. A sealed question card carries its `askText` (what a
//! listener's agent reads, sealed to the own tree), and a sealed line its
//! `listenerCopies`: the text re-sealed for every listening tree outside
//! the own one (`sessions.listeners` once, then each sealed reply's
//! `listeners`).

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use api::session_device::{ListenerCopy, ListenerTree, ReplyCard, ReplyExtras, ReplyResult};
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

/// EXP-1458: what a reply posts as: its body and card sealed to the
/// session's tree (or plaintext when sealing is impossible), plus the card's
/// unsealed JSON. BLOCKING (a first seal may create the tree key).
fn sealed_reply(session_id: &str, reply: &Reply) -> (String, Option<serde_json::Value>, Option<serde_json::Value>) {
    let plain_card = reply.card.as_ref().and_then(|card| match serde_json::to_value(card) {
        Ok(value) => Some(value),
        Err(err) => {
            log::warn!("engine: reply card of {session_id} not serializable: {err}");
            None
        }
    });
    let body = sealing::seal_body_or_plain(session_id, &reply.body);
    let card = plain_card.as_ref().map(|card| sealing::seal_card_or_plain(session_id, card));
    (body, card, plain_card)
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
        let sealed = sealing::seal_body_or_plain(session_id, text);
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
        listeners.as_deref().unwrap_or_default().iter().filter_map(|tree| listener_copy(tree, line)).collect::<Vec<_>>()
    });
    ReplyExtras {
        ask_text,
        listener_copies: copies.filter(|copies| !copies.is_empty()),
        ..ReplyExtras::default()
    }
}

/// `line` sealed to one listening tree's current key (`None` = a keyless
/// tree: its listeners get the neutral line).
fn listener_copy(tree: &ListenerTree, line: &str) -> Option<ListenerCopy> {
    let (Some(epoch), Some(key)) = (tree.epoch, tree.public_key.as_deref()) else { return None };
    let public = sealing::public_key_from_b64(key).ok()?;
    if let Some(service) = sealing::service() {
        service.keyring().set_current(&tree.root_id, epoch, public);
    }
    match sealing::seal_body_for_tree(&tree.root_id, line) {
        Ok(body) => Some(ListenerCopy { root_id: tree.root_id.clone(), body }),
        Err(err) => {
            log::warn!("engine: no listener copy for tree {}: {err}", tree.root_id);
            None
        }
    }
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

/// One run's outbox. Cheap to share; the workers own clones of the state.
pub struct Outbox {
    replies: flume::Sender<Reply>,
    consumes: flume::Sender<String>,
    shared: Arc<Shared>,
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
    /// Spawn both workers for `session_id`.
    pub fn spawn(api: Arc<dyn ConversationApi>, session_id: &str) -> Outbox {
        let shared = Arc::new(Shared::default());
        let (replies, reply_rx) = flume::unbounded::<Reply>();
        let (consumes, consume_rx) = flume::unbounded::<String>();
        let mut done = Vec::new();

        let (reply_done, reply_done_rx) = flume::bounded(1);
        done.push(reply_done_rx);
        {
            let api = Arc::clone(&api);
            let shared = Arc::clone(&shared);
            let session_id = session_id.to_string();
            spawn_named(&format!("engine-replies-{}", short(&session_id)), move || {
                // EXP-1458: the listening trees, fetched before the first
                // sealed line that needs copies, then kept current off each
                // sealed reply's result.
                let mut listeners: Option<Vec<ListenerTree>> = None;
                while let Some(reply) = next(&reply_rx, &shared.closing) {
                    let (body, card, plain_card) = sealed_reply(&session_id, &reply);
                    let sealed = body.starts_with(sealing::SEALED_BODY_PREFIX)
                        || card.as_ref().is_some_and(|card| card.get("sealed").is_some());
                    let extras = if sealed {
                        reply_extras(api.as_ref(), &session_id, &reply, plain_card.as_ref(), &mut listeners)
                    } else {
                        ReplyExtras::default()
                    };
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
                }
                let _ = reply_done.send(());
            });
        }

        let (consume_done, consume_done_rx) = flume::bounded(1);
        done.push(consume_done_rx);
        {
            let shared = Arc::clone(&shared);
            let session_id = session_id.to_string();
            spawn_named(&format!("engine-consume-{}", short(&session_id)), move || {
                while let Some(first) = next(&consume_rx, &shared.closing) {
                    let mut ids = vec![first];
                    ids.extend(consume_rx.drain());
                    ids.dedup();
                    let settled = consume_batch(api.as_ref(), &shared.closing, &session_id, ids);
                    if let Ok(mut unconsumed) = shared.unconsumed.lock() {
                        for id in &settled {
                            unconsumed.remove(id);
                        }
                    }
                }
                let _ = consume_done.send(());
            });
        }

        Outbox {
            replies,
            consumes,
            shared,
            done,
        }
    }

    /// Queue rows for the Conversation, in order.
    pub fn post(&self, replies: Vec<Reply>) {
        for reply in replies {
            let _ = self.replies.send(reply);
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

/// The next queued item; `None` once the outbox is closing and the queue is
/// empty (or every sender is gone).
fn next<T>(rx: &flume::Receiver<T>, closing: &AtomicBool) -> Option<T> {
    loop {
        match rx.recv_timeout(POLL) {
            Ok(item) => return Some(item),
            Err(flume::RecvTimeoutError::Disconnected) => return None,
            Err(flume::RecvTimeoutError::Timeout) => {
                if closing.load(Ordering::SeqCst) && rx.is_empty() {
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

fn spawn_named(name: &str, body: impl FnOnce() + Send + 'static) {
    if let Err(err) = std::thread::Builder::new().name(name.to_string()).spawn(body) {
        log::error!("engine: could not start {name}: {err}");
    }
}

fn short(session_id: &str) -> &str {
    session_id.get(..8).unwrap_or(session_id)
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
        fn ensure_key(&self, root_id: &str) -> Result<(u32, [u8; 32]), String> {
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

    #[derive(Debug, Clone, PartialEq)]
    enum Call {
        Reply(String, Option<serde_json::Value>),
        Consume(Vec<String>),
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
        let outbox = Outbox::spawn(fake.clone(), "sess-1");
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

    /// A transient failure is retried and nothing behind it overtakes it.
    #[test]
    fn a_failed_reply_is_retried_before_the_next_one_posts() {
        let fake = Arc::new(Fake::default());
        *fake.fail_replies.lock().unwrap() = 1;
        let outbox = Outbox::spawn(fake.clone(), "sess-1");
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
        let outbox = Outbox::spawn(fake.clone(), "sess-1");
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
        let outbox = Outbox::spawn(fake.clone(), "sess-1");
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
        let outbox = Outbox::spawn(Arc::new(Down), "sess-1");
        outbox.consume(["m-2".to_string(), "m-1".to_string()]);
        outbox.close(Duration::from_secs(5));
        assert_eq!(outbox.unconsumed(), vec!["m-1".to_string(), "m-2".to_string()]);

        let fake = Arc::new(Fake { stale: Some("m-gone".into()), ..Fake::default() });
        let outbox = Outbox::spawn(fake, "sess-1");
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
        let outbox = Outbox::spawn(api.clone(), "sealed-child");
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

    #[test]
    fn only_transient_failures_retry() {
        assert!(retryable(&ApiError::transport("offline")));
        assert!(retryable(&ApiError::Http { status: 502, message: String::new() }));
        assert!(retryable(&ApiError::Http { status: 429, message: String::new() }));
        assert!(!retryable(&ApiError::Http { status: 400, message: String::new() }));
        assert!(!retryable(&ApiError::Unauthorized));
    }
}
