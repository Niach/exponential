//! EXP-1458 (contract addendum 3): the process-wide DIRECT INBOX of
//! agent-to-agent messages that arrived over eNet chat
//! (`steer::enet`), ahead of their Electric copy.
//!
//! The receiving pickup host merges the inbox into its keyed messages right
//! before [`crate::pickup::plan`] ([`merge`]): a direct row is a pending
//! input like any other (sealed, `author_session_id` = the sender), so the
//! plan delivers it at once and its handed/dedup-by-id bookkeeping covers
//! it. The Electric copy is history + fallback: once a row with the same id
//! is synced the inbox copy is dropped for good (the synced one, consumed
//! or not, is the truth from then on), and a copy whose Electric twin never
//! shows is dropped after [`TTL`]. [`on_arrival`] lets the host run a pass
//! the moment a message lands.

use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use domain::rows::MessageRow;

/// How long a direct row waits for its Electric copy before it is dropped.
pub const TTL: Duration = Duration::from_secs(10 * 60);

/// The rows that arrived and the ids already superseded by their synced
/// copy (a late duplicate send never resurrects one).
#[derive(Default)]
pub struct Inbox {
    rows: Vec<(MessageRow, Instant)>,
    synced: HashSet<String>,
}

impl Inbox {
    /// Keeps `row` (by id, first wins). Returns whether it was new.
    pub fn push(&mut self, row: MessageRow, now: Instant) -> bool {
        if row.id.is_empty() || self.synced.contains(&row.id) || self.rows.iter().any(|(r, _)| r.id == row.id) {
            return false;
        }
        self.rows.push((row, now));
        true
    }

    /// Appends every held row whose id is not in `messages`; drops the ones
    /// `messages` already carries (their Electric copy) and the expired.
    pub fn merge(&mut self, messages: &mut Vec<MessageRow>, now: Instant) {
        if self.rows.is_empty() {
            return;
        }
        let present: HashSet<&str> = messages.iter().map(|m| m.id.as_str()).collect();
        let mut keep = Vec::with_capacity(self.rows.len());
        let mut add = Vec::new();
        for (row, at) in self.rows.drain(..) {
            if present.contains(row.id.as_str()) {
                self.synced.insert(row.id);
            } else if now.saturating_duration_since(at) < TTL {
                add.push(row.clone());
                keep.push((row, at));
            }
        }
        self.rows = keep;
        messages.extend(add);
        // The superseded ids only guard against a resend in flight.
        if self.synced.len() > 4096 {
            self.synced.clear();
        }
    }

    /// The held rows (tests, logs).
    pub fn rows(&self) -> Vec<MessageRow> {
        self.rows.iter().map(|(row, _)| row.clone()).collect()
    }
}

type Hook = Arc<dyn Fn() + Send + Sync>;

static INBOX: Mutex<Option<Inbox>> = Mutex::new(None);
static HOOK: Mutex<Option<Hook>> = Mutex::new(None);

fn with_inbox<T>(f: impl FnOnce(&mut Inbox) -> T) -> T {
    let mut guard = INBOX.lock().unwrap_or_else(|p| p.into_inner());
    f(guard.get_or_insert_with(Inbox::default))
}

/// A direct message arrived: keep it and wake the host's pickup.
pub fn push(row: MessageRow) {
    if !with_inbox(|inbox| inbox.push(row, Instant::now())) {
        return;
    }
    let hook = HOOK.lock().unwrap_or_else(|p| p.into_inner()).clone();
    if let Some(hook) = hook {
        hook();
    }
}

/// The pickup host's merge, right before `pickup::plan` ([`Inbox::merge`]).
pub fn merge(messages: &mut Vec<MessageRow>) {
    with_inbox(|inbox| inbox.merge(messages, Instant::now()));
}

/// The rows held now.
pub fn pending() -> Vec<MessageRow> {
    with_inbox(|inbox| inbox.rows())
}

/// The host's "run a pickup pass now" (one per process; the last one set
/// wins). Called off the host's thread: it must only schedule.
pub fn on_arrival(hook: Box<dyn Fn() + Send + Sync>) {
    *HOOK.lock().unwrap_or_else(|p| p.into_inner()) = Some(Arc::from(hook));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(id: &str) -> MessageRow {
        MessageRow {
            id: id.into(),
            session_id: "target".into(),
            root_id: Some("root".into()),
            author_kind: Some("agent".into()),
            author_session_id: Some("sender".into()),
            body: Some("exp-sealed:v1:1:abc".into()),
            ..Default::default()
        }
    }

    #[test]
    fn merge_adds_held_rows_and_dedups_by_id() {
        let now = Instant::now();
        let mut inbox = Inbox::default();
        assert!(inbox.push(row("m1"), now));
        assert!(!inbox.push(row("m1"), now), "a resend is one row");
        let mut messages = vec![row("other")];
        inbox.merge(&mut messages, now);
        assert_eq!(messages.iter().map(|m| m.id.as_str()).collect::<Vec<_>>(), ["other", "m1"]);
        assert!(crate::pickup::is_pending_input(&messages[1]));
        // Still held until the Electric copy shows.
        let mut again = Vec::new();
        inbox.merge(&mut again, now);
        assert_eq!(again.len(), 1);
    }

    #[test]
    fn the_electric_copy_supersedes_the_direct_row_for_good() {
        let now = Instant::now();
        let mut inbox = Inbox::default();
        inbox.push(row("m1"), now);
        let synced = MessageRow { consumed_at: Some("2026-10-30T12:00:00Z".into()), ..row("m1") };
        let mut messages = vec![synced.clone()];
        inbox.merge(&mut messages, now);
        assert_eq!(messages, vec![synced]);
        assert!(inbox.rows().is_empty());
        // A late resend never resurrects the consumed message.
        assert!(!inbox.push(row("m1"), now));
        let mut empty = Vec::new();
        inbox.merge(&mut empty, now);
        assert!(empty.is_empty());
    }

    #[test]
    fn an_unsynced_row_expires_after_the_ttl() {
        let start = Instant::now();
        let mut inbox = Inbox::default();
        inbox.push(row("m1"), start);
        let mut messages = Vec::new();
        inbox.merge(&mut messages, start + TTL - Duration::from_secs(1));
        assert_eq!(messages.len(), 1);
        let mut later = Vec::new();
        inbox.merge(&mut later, start + TTL);
        assert!(later.is_empty());
        assert!(inbox.rows().is_empty());
    }

    #[test]
    fn push_wakes_the_host_once_per_new_row() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let woke = Arc::new(AtomicUsize::new(0));
        let count = Arc::clone(&woke);
        on_arrival(Box::new(move || {
            count.fetch_add(1, Ordering::SeqCst);
        }));
        let id = format!("wake-{}", std::process::id());
        push(row(&id));
        push(row(&id));
        assert_eq!(woke.load(Ordering::SeqCst), 1);
        assert!(pending().iter().any(|m| m.id == id));
        let mut messages = vec![row(&id)];
        merge(&mut messages);
        assert!(!pending().iter().any(|m| m.id == id));
    }
}
