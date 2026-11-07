//! VAPP-235 (O1) / VAPP-272: the record outbox on the Engine. Each account has
//! one `eengine::SyncedRecordStore`: the queue, the flusher, the waiters and
//! the prune live there (`outbox.json` `syncedCases`). This file fills in its
//! providers (the sync sqlite's `app_record_outbox`, tRPC) and connects it to
//! gpui: the observers below keep it fed, and [`RecordOutbox`] is the entity
//! hosts observe (the overlay, the header's pending glyph).

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

use domain::app_records::{OutboxEntry, OutboxResult};
use domain::via_app::ViaAppClaim;
use eengine::providers::{Failure, Pushed, SyncTransport};
use eengine::{Clock, OutboxLog, SyncedListener, SyncedRecordStore};
use eui_core::host::RecordRow;
use gpui::{App, AppContext as _, AsyncApp, Entity};
use serde_json::{json, Value};

/// The account's outbox table, found through the manager on every write: a
/// replay runs off the UI thread and the pipeline may have gone since.
struct SqliteLog {
    manager: Arc<sync::SyncManager>,
    account_id: String,
}

impl SqliteLog {
    fn db(&self) -> Result<Arc<sync::store::ShapeStore>, Failure> {
        self.manager.store(&self.account_id).ok_or_else(|| Failure::new("UNAVAILABLE", "no store for the account"))
    }
}

fn store_failure(err: impl std::fmt::Display) -> Failure {
    Failure::new("INTERNAL", err.to_string())
}

impl OutboxLog for SqliteLog {
    fn entries(&self) -> Result<Vec<OutboxEntry>, Failure> {
        self.db()?.outbox_entries().map_err(store_failure)
    }

    fn push(&self, entry: &OutboxEntry) -> Result<(), Failure> {
        self.db()?.outbox_push(entry).map_err(store_failure)
    }

    fn ack(&self, write_id: &str, at: &str) -> Result<(), Failure> {
        self.db()?.outbox_ack(write_id, at).map_err(store_failure)
    }

    fn remove(&self, write_ids: &[String]) -> Result<(), Failure> {
        let ids: Vec<&str> = write_ids.iter().map(String::as_str).collect();
        self.db()?.outbox_remove(&ids).map_err(store_failure)
    }
}

/// tRPC out, as the entry's account and app. The refusals keep their
/// [`api::ApiError`] for the toast and the record form to read.
struct TrpcTransport {
    trpc: api::TrpcClient,
    /// VAPP-231 (via-app.json `claim`): each app's published version, the
    /// synced row at send time, off the UI thread (the server checks it).
    published: Arc<RwLock<HashMap<String, Option<i64>>>>,
    errors: Arc<Mutex<HashMap<String, api::ApiError>>>,
}

impl TrpcTransport {
    fn replay(&self, app: &str, call: &str, input: &Value) -> Result<Value, api::ApiError> {
        let text = |key: &str| input.get(key).and_then(Value::as_str).map(str::to_string);
        let write_id = input.get("writeId").and_then(Value::as_str);
        let via_app = self.claim(app);
        match call {
            "upsert" => api::apps::records_upsert(
                &self.trpc,
                &api::apps::RecordUpsertInput {
                    id: app.to_string(),
                    kind: text("kind").unwrap_or_default(),
                    data: input.get("data").cloned().unwrap_or_else(|| json!({})),
                    record_id: text("recordId"),
                    author: None,
                    via: None,
                    new_id: text("newId"),
                    write_id: text("writeId"),
                    via_app,
                },
            )
            .map(|out| json!({ "id": out.id })),
            "delete" => api::apps::records_delete(&self.trpc, app, &text("recordId").unwrap_or_default(), None, write_id).map(|_| Value::Null),
            _ => {
                let writes = input.get("writes").and_then(Value::as_array).cloned().unwrap_or_default();
                api::apps::records_batch(&self.trpc, app, &writes, write_id, via_app.as_ref()).map(|out| json!({ "ids": out.ids, "eventIds": out.event_ids }))
            }
        }
    }

    fn claim(&self, app: &str) -> Option<ViaAppClaim> {
        let published = *self.published.read().ok()?.get(app)?;
        domain::via_app::via_app_claim(Some(app), published)
    }
}

impl SyncTransport for TrpcTransport {
    fn push(&self, app: &str, call: &str, input: &Value) -> OutboxResult {
        self.send(app, call, input).result
    }

    /// A server code, else no network.
    fn send(&self, app: &str, call: &str, input: &Value) -> Pushed {
        match self.replay(app, call, input) {
            Ok(answer) => Pushed { answer, ..Pushed::of(OutboxResult::Ok) },
            Err(err) => {
                let result = err.trpc_code().map_or(OutboxResult::Transport, |code| OutboxResult::Code(code.to_string()));
                let pushed = Pushed { message: Some(err.to_string()), ..Pushed::of(result) };
                if let (Some(write_id), Ok(mut errors)) = (input.get("writeId").and_then(Value::as_str), self.errors.lock()) {
                    errors.insert(write_id.to_string(), err);
                }
                pushed
            }
        }
    }
}

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

struct WallClock;

impl Clock for WallClock {
    fn now(&self) -> String {
        crate::exponential_ui_host::now_iso()
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
    let auth = cx.try_global::<crate::session::AuthContext>()?;
    let account = auth.auth.account(account_id)?;
    let provider: Arc<dyn api::TokenProvider> = auth.auth.token_provider(&account.id);
    let trpc = api::TrpcClient::new(&account.instance_url, provider);
    let errors: Arc<Mutex<HashMap<String, api::ApiError>>> = Arc::default();
    let published = outbox.read(cx).published.clone();
    let transport = Arc::new(TrpcTransport { trpc, published, errors: errors.clone() });
    let log = Arc::new(SqliteLog { manager, account_id: account_id.to_string() });
    let (tx, rx) = flume::unbounded();
    let store = SyncedRecordStore::new(log, transport, Arc::new(WallClock), Some(Arc::new(Listener(tx))))
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

#[cfg(test)]
mod tests {
    use super::*;
    use api::trpc::{Blob, CallKind, Transport};

    /// Answers every call with `reply`, keeping what was sent.
    struct Fake {
        reply: fn() -> Result<String, api::ApiError>,
        sent: Mutex<Vec<(String, Value)>>,
    }

    impl Transport for Fake {
        fn call(&self, _: CallKind, path: &str, input: Option<&str>, _: Option<&str>) -> Result<String, api::ApiError> {
            self.sent.lock().unwrap().push((path.to_string(), serde_json::from_str(input.unwrap_or("null")).unwrap()));
            (self.reply)()
        }
        fn get_blob(&self, _: &str, _: Duration) -> Result<Blob, api::ApiError> {
            unreachable!()
        }
        fn put_bytes(&self, _: &str, _: Vec<u8>, _: &str) -> Result<(), api::ApiError> {
            unreachable!()
        }
        fn post_json(&self, _: &str, _: &str, _: &str, _: Duration) -> Result<String, api::ApiError> {
            unreachable!()
        }
        fn auth_headers(&self) -> Vec<(String, String)> {
            Vec::new()
        }
        fn base_url(&self) -> &str {
            ""
        }
    }

    fn transport(reply: fn() -> Result<String, api::ApiError>) -> (TrpcTransport, Arc<Fake>) {
        let fake = Arc::new(Fake { reply, sent: Mutex::default() });
        let published = HashMap::from([("app".to_string(), Some(3))]);
        let transport = TrpcTransport {
            trpc: api::TrpcClient::with_transport(fake.clone()),
            published: Arc::new(RwLock::new(published)),
            errors: Arc::default(),
        };
        (transport, fake)
    }

    #[test]
    fn a_batch_carries_its_write_id_claim_and_undo_events() {
        let (transport, fake) = transport(|| Ok(json!({"ids": ["r1"], "eventIds": ["e1"]}).to_string()));
        let input = json!({"writes": [{"op": "delete", "id": "r1"}], "writeId": "w1"});
        let pushed = transport.send("app", "batch", &input);
        assert_eq!(pushed.result, OutboxResult::Ok);
        assert_eq!(pushed.answer, json!({"ids": ["r1"], "eventIds": ["e1"]}));
        let sent = fake.sent.lock().unwrap();
        assert_eq!(sent[0].0, "apps.records.batch");
        assert_eq!(sent[0].1["writeId"], "w1");
        assert_eq!(sent[0].1["viaApp"]["version"], 3);
    }

    #[test]
    fn a_refusal_keeps_its_typed_error_for_the_form() {
        let (transport, _) = transport(|| Err(api::ApiError::Http { status: 400, message: "title".into() }));
        let pushed = transport.send("app", "upsert", &json!({"kind": "task", "data": {}, "newId": "r1", "writeId": "w1"}));
        assert_eq!(pushed.result, OutboxResult::Code("BAD_REQUEST".into()));
        assert!(matches!(transport.errors.lock().unwrap().remove("w1"), Some(api::ApiError::Http { status: 400, .. })));
    }

    #[test]
    fn no_network_reads_as_a_retry() {
        let (transport, _) = transport(|| Err(api::ApiError::transport("refused")));
        let pushed = transport.send("app", "delete", &json!({"recordId": "r1", "writeId": "w1"}));
        assert_eq!(pushed.result, OutboxResult::Transport);
    }
}
