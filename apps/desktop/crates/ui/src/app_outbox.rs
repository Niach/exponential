//! VAPP-235 (O1) / VAPP-272: the record outbox on the Engine. Each account has
//! one `eengine::SyncedRecordStore`: the queue, the flusher, the waiters and
//! the prune live there (`outbox.json` `syncedCases`). This file fills in its
//! providers (the sync sqlite's `app_record_outbox`, tRPC) and connects it to
//! gpui: the observers below keep it fed, and [`RecordOutbox`] is the entity
//! hosts observe (the overlay, the header's pending glyph).

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

use domain::app_records::OutboxEntry;
use eengine::providers::{Pushed, SystemClock};
use eengine::{SyncedListener, SyncedRecordStore};
use eui_core::host::RecordRow;
use gpui::{App, AppContext as _, AsyncApp, Entity};
use sync::app_outbox::{SqliteLog, TrpcTransport};

/// What the queue tells the UI thread.
enum Heard {
    Changed,
    Refused { write_id: String, verb: String },
}

struct Listener(flume::Sender<Heard>);

impl SyncedListener for Listener {
    fn changed(&self) {
        let _ = self.0.send(Heard::Changed);
    }

    fn refused(&self, entry: &OutboxEntry, verb: &str, _pushed: &Pushed) {
        let _ = self.0.send(Heard::Refused { write_id: entry.write_id.clone(), verb: verb.to_string() });
    }
}

/// One account's records: the Engine's store and the failures it replayed into.
#[derive(Clone)]
pub(crate) struct AccountRecords {
    pub(crate) store: Arc<SyncedRecordStore>,
    errors: Arc<Mutex<HashMap<String, api::ApiError>>>,
}

impl AccountRecords {
    /// The typed failure of `write_id`'s last replay (once).
    pub(crate) fn take_error(&self, write_id: &str) -> Option<api::ApiError> {
        self.errors.lock().ok()?.remove(write_id)
    }
}

/// The accounts' stores. Hosts observe it. `graced` = the acked entries with
/// their grace timer running.
#[derive(Default)]
pub(crate) struct RecordOutbox {
    accounts: HashMap<String, AccountRecords>,
    published: Arc<RwLock<HashMap<String, Option<i64>>>>,
    graced: HashSet<String>,
}

struct RecordOutboxGlobal(Entity<RecordOutbox>);

impl gpui::Global for RecordOutboxGlobal {}

/// The outbox entity (`None` before [`install_record_outbox`]: tests).
/// #EXP-1727: a Local desktop reopened has a new executor: its queue must
/// not keep flushing through the old one.
pub(crate) fn forget_account(account_id: &str, cx: &mut App) {
    if let Some(outbox) = record_outbox(cx) {
        outbox.update(cx, |o, _| {
            o.accounts.remove(account_id);
        });
    }
}

pub(crate) fn record_outbox(cx: &App) -> Option<Entity<RecordOutbox>> {
    cx.try_global::<RecordOutboxGlobal>().map(|g| g.0.clone())
}

/// An account's records, opened on first use (`None`: no sync store, the
/// account signed out, or its log does not read).
pub(crate) fn account_records(account_id: &str, cx: &mut App) -> Option<AccountRecords> {
    let outbox = record_outbox(cx)?;
    if let Some(records) = outbox.read(cx).accounts.get(account_id) {
        return Some(records.clone());
    }
    let manager = sync::Store::try_global(cx)?.manager();
    // #EXP-1727: a Local desktop's queue flushes into its executor.
    let account = crate::local_desktops::account(account_id, cx).or_else(|| cx.try_global::<crate::session::AuthContext>()?.auth.account(account_id))?;
    let trpc = crate::queries::client_for(&account, cx)?;
    let errors: Arc<Mutex<HashMap<String, api::ApiError>>> = Arc::default();
    let published = outbox.read(cx).published.clone();
    let transport = Arc::new(TrpcTransport::new(trpc, published, errors.clone()));
    let log = Arc::new(SqliteLog::new(manager, account_id));
    let (tx, rx) = flume::unbounded();
    let store = SyncedRecordStore::new(log, transport, Arc::new(SystemClock), Some(Arc::new(Listener(tx))))
        .map_err(|err| log::warn!("[exponential-ui host] the record outbox does not read: {err}"))
        .ok()?;
    let records = AccountRecords { store: Arc::new(store), errors };
    outbox.update(cx, |o, cx| {
        o.accounts.insert(account_id.to_string(), records.clone());
        cx.notify();
    });
    let heard = records.clone();
    cx.spawn(async move |cx: &mut AsyncApp| {
        while let Ok(event) = rx.recv_async().await {
            cx.update(|cx| hear(&heard, event, cx));
        }
    })
    .detach();
    Some(records)
}

fn hear(records: &AccountRecords, event: Heard, cx: &mut App) {
    match event {
        Heard::Changed => {
            notify(cx);
            arm_grace(records, cx);
        }
        // No call awaits it (a later replay): the failed-action toast.
        Heard::Refused { write_id, verb } => {
            if let Some(err) = records.take_error(&write_id) {
                crate::action_error::failed_action_err(&verb, "the record", &err, cx);
            }
        }
    }
}

/// An acked entry the shape never reflects leaves after the grace.
fn arm_grace(records: &AccountRecords, cx: &mut App) {
    let Some(outbox) = record_outbox(cx) else { return };
    let acked: Vec<String> = records.store.entries().into_iter().filter(|e| !e.is_pending()).map(|e| e.write_id).collect();
    let new: Vec<String> = outbox.update(cx, |o, _| acked.into_iter().filter(|id| o.graced.insert(id.clone())).collect());
    for write_id in new {
        let store = records.store.clone();
        let grace = Duration::from_secs(domain::app_records::APP_RECORDS_OUTBOX.ack_grace_seconds + 1);
        cx.spawn(async move |cx: &mut AsyncApp| {
            cx.background_executor().timer(grace).await;
            cx.update(|cx| {
                store.prune();
                if let Some(outbox) = record_outbox(cx) {
                    outbox.update(cx, |o, _| o.graced.remove(&write_id));
                }
            });
        })
        .detach();
    }
}

/// Tell the observers the entries moved.
pub(crate) fn notify(cx: &mut App) {
    if let Some(outbox) = record_outbox(cx) {
        outbox.update(cx, |_, cx| cx.notify());
    }
}

/// Run the flusher off the UI thread (one per account: a second call only
/// asks for another pass).
pub(crate) fn flush(store: &Arc<SyncedRecordStore>, cx: &App) {
    let store = store.clone();
    cx.background_spawn(async move { store.flush() }).detach();
}

/// The active account's entries (none before they load).
pub(crate) fn active_entries(cx: &App) -> Vec<OutboxEntry> {
    let (Some(outbox), Some(account)) = (record_outbox(cx), crate::queries::active_account(cx)) else {
        return Vec::new();
    };
    outbox.read(cx).accounts.get(&account.id).map(|r| r.store.entries()).unwrap_or_default()
}

/// The running app's header glyph label (`outboxPendingLabel`): `None` =
/// nothing pending for `app_id`.
pub(crate) fn pending_label_for(app_id: &str, cx: &App) -> Option<String> {
    domain::app_records::outbox_pending_label(&active_entries(cx), app_id)
}

/// Each app's published version off the store, for the replays' claims.
fn refresh_published(cx: &App) {
    let (Some(outbox), Some(store)) = (record_outbox(cx), sync::Store::try_global(cx)) else { return };
    let next: HashMap<String, Option<i64>> = store.collections().apps.read(cx).iter().map(|a| (a.id.clone(), a.published_version)).collect();
    if let Ok(mut published) = outbox.read(cx).published.write() {
        *published = next;
    }
}

/// The active account's synced rows into its store: acked entries they
/// reflect leave.
fn feed_rows(account_id: &str, cx: &mut App) {
    if crate::queries::active_account(cx).map(|a| a.id).as_deref() != Some(account_id) {
        return;
    }
    let (Some(records), Some(store)) = (account_records(account_id, cx), sync::Store::try_global(cx)) else { return };
    let mut rows: HashMap<String, Vec<RecordRow>> = HashMap::new();
    for r in store.collections().app_records.read(cx).iter() {
        let lite = crate::exponential_ui_host::record_lite(r);
        rows.entry(lite.app_id).or_default().push(RecordRow {
            id: lite.id,
            kind: lite.kind,
            data: lite.data,
            created_by: lite.created_by,
            author_id: lite.author_id,
            created_at: lite.created_at,
            updated_at: lite.updated_at,
        });
    }
    records.store.set_all_rows(rows);
}

/// Install the outbox (after the store and the `AuthContext`, before the
/// session bootstrap): an account's queue opens and flushes at launch /
/// sign-in, and flushes again when the sync connection comes back (the
/// store's health Offline → Ok); the store's rows follow the shape.
pub fn install_record_outbox(cx: &mut App) {
    let Some(store) = sync::Store::try_global(cx).cloned() else { return };
    let outbox = cx.new(|_| RecordOutbox::default());
    cx.set_global(RecordOutboxGlobal(outbox));
    refresh_published(cx);
    let mut last: (Option<String>, bool) = (None, true);
    cx.observe(&store.state(), move |state, cx| {
        let (account, online) = {
            let s = state.read(cx);
            (s.session.account_id().map(str::to_string), s.published_health == sync::SyncHealth::Ok)
        };
        let (was_account, was_online) = std::mem::replace(&mut last, (account.clone(), online));
        let Some(account) = account else { return };
        let switched = was_account.as_deref() != Some(account.as_str());
        if switched || (online && !was_online) {
            if let Some(records) = account_records(&account, cx) {
                if switched {
                    notify(cx);
                    feed_rows(&account, cx);
                    arm_grace(&records, cx);
                }
                flush(&records.store, cx);
            }
        }
    })
    .detach();
    cx.observe(&store.collections().app_records, |_, cx| {
        if let Some(account) = crate::queries::active_account(cx) {
            feed_rows(&account.id, cx);
        }
    })
    .detach();
    cx.observe(&store.collections().apps, |_, cx| refresh_published(cx)).detach();
}
