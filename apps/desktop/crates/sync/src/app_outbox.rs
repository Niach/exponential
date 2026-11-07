//! VAPP-272 / #VAPP-273: the Engine's synced-store providers every
//! connected host shares (the IDE's apps host, Exponential Desktop's shell
//! on a connected team): the account's `app_record_outbox` table as the
//! [`OutboxLog`], tRPC `apps.records.*` as the [`SyncTransport`] (the
//! refusals keep their typed [`api::ApiError`] for the UI to read), and the
//! published versions the via-app claim needs. No gpui here: the hosts
//! connect them to their own loop.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, RwLock};

use domain::app_records::{OutboxEntry, OutboxResult};
use domain::via_app::ViaAppClaim;
use eengine::providers::{Failure, Pushed, SyncTransport};
use eengine::OutboxLog;
use serde_json::{json, Value};

/// Each app's published version (the server's app id → version), the
/// synced row at send time; what the via-app claim carries.
pub type PublishedVersions = Arc<RwLock<HashMap<String, Option<i64>>>>;
/// The typed failure of a write id's last replay, for the toast and the form.
pub type ReplayErrors = Arc<Mutex<HashMap<String, api::ApiError>>>;

/// The account's outbox table, found through the manager on every write: a
/// replay runs off the UI thread and the pipeline may have gone since.
pub struct SqliteLog {
    manager: Arc<crate::SyncManager>,
    account_id: String,
}

impl SqliteLog {
    pub fn new(manager: Arc<crate::SyncManager>, account_id: impl Into<String>) -> Self {
        SqliteLog { manager, account_id: account_id.into() }
    }

    fn db(&self) -> Result<Arc<crate::store::ShapeStore>, Failure> {
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
pub struct TrpcTransport {
    trpc: api::TrpcClient,
    /// VAPP-231 (via-app.json `claim`): each app's published version, the
    /// synced row at send time, off the UI thread (the server checks it).
    published: PublishedVersions,
    errors: ReplayErrors,
}

impl TrpcTransport {
    pub fn new(trpc: api::TrpcClient, published: PublishedVersions, errors: ReplayErrors) -> Self {
        TrpcTransport { trpc, published, errors }
    }

    pub fn errors(&self) -> &ReplayErrors {
        &self.errors
    }

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


#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

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
