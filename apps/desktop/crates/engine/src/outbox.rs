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

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use api::session_device::ReplyCard;
use api::ApiError;

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
pub trait ConversationApi: Send + Sync + 'static {
    fn reply(&self, session_id: &str, body: &str, card: Option<&ReplyCard>) -> Result<String, ApiError>;
    fn consume(&self, session_id: &str, message_ids: &[String]) -> Result<(), ApiError>;
}

impl ConversationApi for api::TrpcClient {
    fn reply(&self, session_id: &str, body: &str, card: Option<&ReplyCard>) -> Result<String, ApiError> {
        api::session_device::reply(self, session_id, body, card)
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
                while let Some(reply) = next(&reply_rx, &shared.closing) {
                    let posted = with_retry(&shared.closing, "sessions.reply", false, || {
                        api.reply(&session_id, &reply.body, reply.card.as_ref())
                    });
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

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, PartialEq)]
    enum Call {
        Reply(String, Option<ReplyCard>),
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
    }

    impl ConversationApi for Fake {
        fn reply(&self, _session: &str, body: &str, card: Option<&ReplyCard>) -> Result<String, ApiError> {
            {
                let mut fail = self.fail_replies.lock().unwrap();
                if *fail > 0 {
                    *fail -= 1;
                    return Err(ApiError::Http { status: 503, message: "down".into() });
                }
            }
            self.calls.lock().unwrap().push(Call::Reply(body.to_string(), card.cloned()));
            Ok(format!("msg-{}", self.ids.fetch_add(1, Ordering::SeqCst) + 1))
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
                Call::Reply(String::new(), Some(question("q-1"))),
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
            fn reply(&self, _: &str, _: &str, _: Option<&ReplyCard>) -> Result<String, ApiError> {
                Ok("m".into())
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

    #[test]
    fn only_transient_failures_retry() {
        assert!(retryable(&ApiError::transport("offline")));
        assert!(retryable(&ApiError::Http { status: 502, message: String::new() }));
        assert!(retryable(&ApiError::Http { status: 429, message: String::new() }));
        assert!(!retryable(&ApiError::Http { status: 400, message: String::new() }));
        assert!(!retryable(&ApiError::Unauthorized));
    }
}
