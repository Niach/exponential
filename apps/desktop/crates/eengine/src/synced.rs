//! VAPP-272: the `SyncedRecordStore`, Exponential's record store (spec §14):
//! a cache of the synced rows the host feeds in (Electric, through each
//! platform's own sync client) plus O1's outbox, ONE per account across its
//! apps. The entries persist through the host's [`OutboxLog`] (desktop sync
//! sqlite, GRDB, Room) and replay through its [`SyncTransport`] (tRPC); the
//! server stays the authority. `outbox.json` `syncedCases` is the contract.
//!
//! [`submit`](SyncedRecordStore::submit) and [`flush`](SyncedRecordStore::flush)
//! BLOCK on the transport: a host calls them off its UI thread. Reads never
//! wait for a replay.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, RwLock};

use eui_core::host::{RecordRow, RecordsDelta};
use serde_json::Value;

use crate::instance::{outbox_row, overlay};
use crate::outbox::{outbox_outcome, outbox_pending_label, outbox_prune, outbox_request, outbox_verb, OutboxEntry, OutboxOutcome, OutboxState};
use crate::providers::{Clock, Failure, Pushed, RecordEvent, RecordStore, Subscription, SyncTransport, WriteAck, WriteBatch};

/// Where an account's entries persist, oldest first. A row that no longer
/// parses is the log's to skip or remove.
pub trait OutboxLog: Send + Sync {
    fn entries(&self) -> Result<Vec<OutboxEntry>, Failure>;
    fn push(&self, entry: &OutboxEntry) -> Result<(), Failure>;
    fn ack(&self, write_id: &str, at: &str) -> Result<(), Failure>;
    fn remove(&self, write_ids: &[String]) -> Result<(), Failure>;
}

/// What a host hears from the queue. Both run on the thread that replays.
pub trait SyncedListener: Send + Sync {
    /// The entries moved (the overlay, the header's pending glyph).
    fn changed(&self) {}
    /// A replay nobody awaits was refused: the failed-action toast.
    fn refused(&self, _entry: &OutboxEntry, _verb: &str, _pushed: &Pushed) {}
}

/// A host call's answer.
#[derive(Clone, Debug, PartialEq)]
pub enum Submitted {
    /// The server took it: its answer.
    Done(Value),
    /// The server refused it (the entry is gone): the call fails.
    Refused(Pushed),
    /// Offline, behind others, a retry or a drop: answer from the entry.
    Queued,
}

type Listener = Arc<dyn Fn(RecordsDelta) + Send + Sync>;

#[derive(Default)]
struct Queue {
    entries: Vec<OutboxEntry>,
    flushing: bool,
    /// A flush was asked for while one ran: one more pass.
    again: bool,
    /// Write ids a host call awaits, then their first outcome.
    awaited: HashSet<String>,
    outcomes: HashMap<String, Submitted>,
}

#[derive(Default)]
struct Rows {
    apps: HashMap<String, Vec<RecordRow>>,
    /// Set by [`SyncedRecordStore::set_all_rows`]: an app not in `apps` has no rows.
    complete: bool,
}

/// One account's records.
pub struct SyncedRecordStore {
    log: Arc<dyn OutboxLog>,
    transport: Arc<dyn SyncTransport>,
    clock: Arc<dyn Clock>,
    listener: Option<Arc<dyn SyncedListener>>,
    queue: Mutex<Queue>,
    settled: Condvar,
    rows: RwLock<Rows>,
    subscribers: Arc<Mutex<HashMap<String, Vec<(u64, Listener)>>>>,
    next_subscriber: Mutex<u64>,
}

impl SyncedRecordStore {
    /// Reads the log once: a restart picks the queue up where it stopped.
    pub fn new(log: Arc<dyn OutboxLog>, transport: Arc<dyn SyncTransport>, clock: Arc<dyn Clock>, listener: Option<Arc<dyn SyncedListener>>) -> Result<Self, Failure> {
        let entries = log.entries()?;
        Ok(SyncedRecordStore {
            log,
            transport,
            clock,
            listener,
            queue: Mutex::new(Queue { entries, ..Queue::default() }),
            settled: Condvar::new(),
            rows: RwLock::new(Rows::default()),
            subscribers: Arc::default(),
            next_subscriber: Mutex::new(0),
        })
    }

    fn locked(&self) -> MutexGuard<'_, Queue> {
        self.queue.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn changed(&self) {
        if let Some(listener) = &self.listener {
            listener.changed();
        }
    }

    /// Every entry, oldest first.
    pub fn entries(&self) -> Vec<OutboxEntry> {
        self.locked().entries.clone()
    }

    /// The header glyph's label for `app` (`None` = nothing pending).
    pub fn pending_label(&self, app: &str) -> Option<String> {
        outbox_pending_label(&self.locked().entries, app)
    }

    /// `app`'s synced rows with its entries laid over them.
    pub fn visible_rows(&self, app: &str, me: Option<&str>) -> Vec<RecordRow> {
        let rows = self.synced(app);
        let entries: Vec<OutboxEntry> = self.locked().entries.iter().filter(|e| e.app_id == app).cloned().collect();
        if entries.is_empty() {
            rows
        } else {
            overlay(rows, &entries, me)
        }
    }

    fn synced(&self, app: &str) -> Vec<RecordRow> {
        self.rows.read().unwrap_or_else(|e| e.into_inner()).apps.get(app).cloned().unwrap_or_default()
    }

    /// Persist a host call's entry. Alone and `online`, it replays now and
    /// answers its own first outcome; otherwise it answers [`Submitted::Queued`]
    /// with nothing sent, and the host calls [`flush`](Self::flush).
    pub fn submit(&self, entry: OutboxEntry, online: bool) -> Result<Submitted, Failure> {
        let write_id = entry.write_id.clone();
        Ok(if self.enqueue(entry, online)? { self.await_outcome(&write_id) } else { Submitted::Queued })
    }

    /// [`submit`](Self::submit)'s first half, for a host that persists on its
    /// UI thread and replays off it: true = the call awaits its own first
    /// outcome ([`await_outcome`](Self::await_outcome)). Never touches the transport.
    pub fn enqueue(&self, entry: OutboxEntry, online: bool) -> Result<bool, Failure> {
        let awaits = {
            let mut q = self.locked();
            self.log.push(&entry)?;
            let awaits = online && !q.entries.iter().any(OutboxEntry::is_pending);
            if awaits {
                q.awaited.insert(entry.write_id.clone());
            }
            q.entries.push(entry);
            awaits
        };
        self.changed();
        Ok(awaits)
    }

    /// [`submit`](Self::submit)'s second half: replay, and answer the entry
    /// [`enqueue`](Self::enqueue) said to await.
    pub fn await_outcome(&self, write_id: &str) -> Submitted {
        self.flush();
        // Another thread's flush may be the one replaying it.
        let mut q = self.locked();
        while q.flushing && !q.outcomes.contains_key(write_id) {
            q = self.settled.wait(q).unwrap_or_else(|e| e.into_inner());
        }
        q.awaited.remove(write_id);
        q.outcomes.remove(write_id).unwrap_or(Submitted::Queued)
    }

    /// Replay the pending entries oldest first, one at a time, until none is
    /// left or one must retry. One flusher at a time: a call while one runs
    /// asks it for another pass and returns.
    pub fn flush(&self) {
        {
            let mut q = self.locked();
            if q.flushing {
                q.again = true;
                return;
            }
            q.flushing = true;
        }
        loop {
            let next = self.locked().entries.iter().find(|e| e.is_pending()).cloned();
            let stop = match next {
                Some(entry) => {
                    let (call, input) = outbox_request(&entry);
                    let pushed = self.transport.send(&entry.app_id, call.as_str(), &input);
                    self.settle(&entry, pushed)
                }
                None => true,
            };
            if stop {
                let mut q = self.locked();
                let empty = !q.entries.iter().any(OutboxEntry::is_pending);
                if !q.again || empty {
                    q.again = false;
                    q.flushing = false;
                    self.settled.notify_all();
                    return;
                }
                q.again = false;
            }
        }
    }

    /// Apply one replay's outcome (`outboxOutcome`); true = the flush stops.
    /// A log that fails to write keeps the entry there: a replay carries the
    /// same `writeId`, which the server answers without writing twice.
    fn settle(&self, entry: &OutboxEntry, pushed: Pushed) -> bool {
        let outcome = outbox_outcome(entry, &pushed.result);
        let mut unawaited_refusal = None;
        {
            let mut q = self.locked();
            let answer = match outcome {
                OutboxOutcome::Done => {
                    let now = self.clock.now();
                    let _ = self.log.ack(&entry.write_id, &now);
                    if let Some(e) = q.entries.iter_mut().find(|e| e.write_id == entry.write_id) {
                        e.state = OutboxState::Acked;
                        e.acked_at = Some(now);
                    }
                    Submitted::Done(pushed.answer.clone())
                }
                OutboxOutcome::Retry => Submitted::Queued,
                OutboxOutcome::Dropped | OutboxOutcome::Refused => {
                    let _ = self.log.remove(std::slice::from_ref(&entry.write_id));
                    q.entries.retain(|e| e.write_id != entry.write_id);
                    if outcome == OutboxOutcome::Refused {
                        Submitted::Refused(pushed.clone())
                    } else {
                        Submitted::Queued
                    }
                }
            };
            if q.awaited.remove(&entry.write_id) {
                q.outcomes.insert(entry.write_id.clone(), answer);
                self.settled.notify_all();
            } else if outcome == OutboxOutcome::Refused {
                unawaited_refusal = Some(pushed);
            }
        }
        if let (Some(pushed), Some(listener)) = (unawaited_refusal, &self.listener) {
            listener.refused(entry, outbox_verb(entry), &pushed);
        }
        if outcome == OutboxOutcome::Done {
            self.prune();
        }
        self.changed();
        outcome == OutboxOutcome::Retry
    }

    /// Feed `app`'s synced rows in: acked entries they reflect leave.
    pub fn set_rows(&self, app: &str, rows: Vec<RecordRow>) {
        let before = {
            let mut cache = self.rows.write().unwrap_or_else(|e| e.into_inner());
            cache.apps.insert(app.to_string(), rows.clone()).unwrap_or_default()
        };
        self.publish(app, &before, &rows);
        self.prune();
    }

    /// Feed in every app's synced rows at once (an app left out has none).
    pub fn set_all_rows(&self, rows: HashMap<String, Vec<RecordRow>>) {
        let before = {
            let mut cache = self.rows.write().unwrap_or_else(|e| e.into_inner());
            cache.complete = true;
            std::mem::replace(&mut cache.apps, rows.clone())
        };
        let subscribed: Vec<String> = self.subscribers.lock().unwrap_or_else(|e| e.into_inner()).keys().cloned().collect();
        for app in subscribed {
            let none = Vec::new();
            self.publish(&app, before.get(&app).unwrap_or(&none), rows.get(&app).unwrap_or(&none));
        }
        self.prune();
    }

    fn publish(&self, app: &str, before: &[RecordRow], rows: &[RecordRow]) {
        let listeners: Vec<Listener> =
            self.subscribers.lock().unwrap_or_else(|e| e.into_inner()).get(app).map(|l| l.iter().map(|(_, on)| on.clone()).collect()).unwrap_or_default();
        if listeners.is_empty() {
            return;
        }
        let upserts: Vec<RecordRow> = rows.iter().filter(|r| !before.iter().any(|b| b == *r)).cloned().collect();
        let deletes: Vec<String> = before.iter().filter(|b| !rows.iter().any(|r| r.id == b.id)).map(|b| b.id.clone()).collect();
        if upserts.is_empty() && deletes.is_empty() {
            return;
        }
        for on in listeners {
            on(RecordsDelta { upserts: upserts.clone(), deletes: deletes.clone() });
        }
    }

    /// Drop the acked entries the synced rows reflect, or past the grace
    /// (`outboxPrune`). An app whose rows were never fed in keeps its entries:
    /// an empty cache would read as every delete reflected.
    pub fn prune(&self) {
        let now = self.clock.now();
        let gone: Vec<String> = {
            let cache = self.rows.read().unwrap_or_else(|e| e.into_inner());
            let q = self.locked();
            let apps: HashSet<&str> = q.entries.iter().filter(|e| !e.is_pending()).map(|e| e.app_id.as_str()).collect();
            let mut gone = Vec::new();
            for app in apps {
                let rows = match cache.apps.get(app) {
                    Some(rows) => rows.as_slice(),
                    None if cache.complete => &[],
                    None => continue,
                };
                let rows: Vec<_> = rows.iter().cloned().map(outbox_row).collect();
                let entries: Vec<OutboxEntry> = q.entries.iter().filter(|e| e.app_id == app).cloned().collect();
                let kept: HashSet<String> = outbox_prune(&entries, &rows, &now).into_iter().map(|e| e.write_id).collect();
                gone.extend(entries.into_iter().map(|e| e.write_id).filter(|id| !kept.contains(id)));
            }
            gone
        };
        if gone.is_empty() {
            return;
        }
        let _ = self.log.remove(&gone);
        self.locked().entries.retain(|e| !gone.contains(&e.write_id));
        self.changed();
    }
}

impl RecordStore for SyncedRecordStore {
    fn rows(&self, app: &str) -> Result<Vec<RecordRow>, Failure> {
        Ok(self.synced(app))
    }

    /// Writes go through the outbox; only the server writes the synced rows.
    fn write(&self, _app: &str, _batch: &WriteBatch, _by: Option<&str>) -> Result<WriteAck, Failure> {
        Err(Failure::new("BAD_REQUEST", "a synced store writes through its outbox"))
    }

    /// The server keeps the history.
    fn history(&self, _app: &str, _id: &str) -> Result<Vec<RecordEvent>, Failure> {
        Ok(Vec::new())
    }

    fn subscribe(&self, app: &str, on: Box<dyn Fn(RecordsDelta) + Send + Sync>) -> Subscription {
        let id = {
            let mut next = self.next_subscriber.lock().unwrap_or_else(|e| e.into_inner());
            *next += 1;
            *next
        };
        self.subscribers.lock().unwrap_or_else(|e| e.into_inner()).entry(app.to_string()).or_default().push((id, Arc::from(on)));
        let (subscribers, app) = (self.subscribers.clone(), app.to_string());
        Subscription(Some(Box::new(move || {
            let mut all = subscribers.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(listeners) = all.get_mut(&app) {
                listeners.retain(|(i, _)| *i != id);
                if listeners.is_empty() {
                    all.remove(&app);
                }
            }
        })))
    }
}
