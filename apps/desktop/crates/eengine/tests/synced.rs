//! VAPP-272: `outbox.json` `syncedCases` replayed on the `SyncedRecordStore`.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use eengine::contract::OUTBOX;
use eengine::outbox::{OutboxEntry, OutboxResult, OutboxState};
use eengine::providers::{Clock, Failure, Pushed, RecordStore, SyncTransport};
use eengine::outbox::OutboxCall;
use eengine::synced::{record_call, OutboxLog, Submitted, SyncedListener, SyncedRecordStore};
use serde_json::{json, Value};

#[derive(Default)]
struct Log(Mutex<Vec<OutboxEntry>>);

impl OutboxLog for Log {
    fn entries(&self) -> Result<Vec<OutboxEntry>, Failure> {
        Ok(self.0.lock().unwrap().clone())
    }
    fn push(&self, entry: &OutboxEntry) -> Result<(), Failure> {
        self.0.lock().unwrap().push(entry.clone());
        Ok(())
    }
    fn ack(&self, write_id: &str, at: &str) -> Result<(), Failure> {
        if let Some(e) = self.0.lock().unwrap().iter_mut().find(|e| e.write_id == write_id) {
            e.state = OutboxState::Acked;
            e.acked_at = Some(at.to_string());
        }
        Ok(())
    }
    fn remove(&self, write_ids: &[String]) -> Result<(), Failure> {
        self.0.lock().unwrap().retain(|e| !write_ids.contains(&e.write_id));
        Ok(())
    }
}

#[derive(Default)]
struct Scripted {
    replies: Mutex<HashMap<String, Vec<Value>>>,
    sent: Mutex<Vec<String>>,
}

impl SyncTransport for Scripted {
    fn push(&self, app: &str, call: &str, input: &Value) -> OutboxResult {
        self.send(app, call, input).result
    }
    fn send(&self, _app: &str, _call: &str, input: &Value) -> Pushed {
        let write_id = input["writeId"].as_str().unwrap().to_string();
        self.sent.lock().unwrap().push(write_id.clone());
        let mut replies = self.replies.lock().unwrap();
        let queue = replies.entry(write_id).or_default();
        let reply = if queue.is_empty() { json!({"transport": true}) } else { queue.remove(0) };
        Pushed {
            result: OutboxResult::from_value(&reply),
            answer: reply.get("answer").cloned().unwrap_or(Value::Null),
            message: reply["message"].as_str().map(str::to_string),
            invalid: reply["invalid"].as_bool().unwrap_or(false),
        }
    }
}

struct Pinned(String);

impl Clock for Pinned {
    fn now(&self) -> String {
        self.0.clone()
    }
}

#[derive(Default)]
struct Refusals(Mutex<Vec<Value>>);

impl SyncedListener for Refusals {
    fn refused(&self, entry: &OutboxEntry, verb: &str, pushed: &Pushed) {
        self.0.lock().unwrap().push(json!({"writeId": entry.write_id, "verb": verb, "message": pushed.message}));
    }
}

fn answer_of(got: Submitted) -> Value {
    match got {
        Submitted::Done(answer) => json!({"done": answer}),
        Submitted::Refused(p) => json!({"refused": {"message": p.message, "invalid": p.invalid}}),
        Submitted::Queued => json!({"queued": true}),
    }
}

#[test]
fn synced_cases_replay() {
    for c in OUTBOX["syncedCases"].as_array().unwrap() {
        let name = c["name"].as_str().unwrap();
        let log = Arc::new(Log(Mutex::new(serde_json::from_value(c["log"].clone()).unwrap())));
        let transport = Arc::new(Scripted::default());
        *transport.replies.lock().unwrap() = serde_json::from_value(c["transport"].clone()).unwrap();
        let refusals = Arc::new(Refusals::default());
        let clock = Arc::new(Pinned(OUTBOX["syncedNow"].as_str().unwrap().to_string()));
        let store = SyncedRecordStore::new(log.clone(), transport.clone(), clock, Some(refusals.clone())).unwrap();
        for step in c["steps"].as_array().unwrap() {
            match step["do"].as_str().unwrap() {
                "submit" => {
                    let entry = serde_json::from_value(step["entry"].clone()).unwrap();
                    let got = store.submit(entry, step["online"].as_bool().unwrap()).unwrap();
                    assert_eq!(answer_of(got), step["expected"], "{name}");
                }
                "flush" => store.flush(),
                "rows" => store.set_rows(step["app"].as_str().unwrap(), serde_json::from_value(step["rows"].clone()).unwrap()),
                "prune" => store.prune(),
                other => panic!("{other}"),
            }
        }
        assert_eq!(json!(*transport.sent.lock().unwrap()), c["sent"], "{name}: sent");
        assert_eq!(json!(*refusals.0.lock().unwrap()), c["refused"], "{name}: refused");
        let left: Vec<Value> = log.0.lock().unwrap().iter().map(|e| json!({"writeId": e.write_id, "state": e.state.as_str()})).collect();
        assert_eq!(json!(left), c["logAfter"], "{name}: log");
        let entries: Vec<Value> = store.entries().iter().map(|e| json!({"writeId": e.write_id, "state": e.state.as_str()})).collect();
        assert_eq!(json!(entries), c["logAfter"], "{name}: entries");
    }
}

/// A transport that waits for the test before it answers.
struct Gate {
    entered: signal::Signal,
    release: signal::Signal,
}

mod signal {
    use std::sync::{Condvar, Mutex};

    #[derive(Default)]
    pub struct Signal(Mutex<bool>, Condvar);

    impl Signal {
        pub fn raise(&self) {
            *self.0.lock().unwrap() = true;
            self.1.notify_all();
        }
        pub fn wait(&self) {
            let mut up = self.0.lock().unwrap();
            while !*up {
                up = self.1.wait(up).unwrap();
            }
        }
    }
}

impl SyncTransport for Gate {
    fn push(&self, _app: &str, _call: &str, _input: &Value) -> OutboxResult {
        self.entered.raise();
        self.release.wait();
        OutboxResult::Ok
    }
}

fn entry(id: &str) -> OutboxEntry {
    serde_json::from_value(json!({"writeId": id, "appId": "a", "call": "upsert", "writes": [{"op": "create", "id": id, "kind": "task", "data": {}}], "at": "2026-11-07T09:00:00.000Z", "state": "pending"})).unwrap()
}

#[test]
fn a_flush_while_one_runs_only_asks_for_another_pass() {
    let gate = Arc::new(Gate { entered: Default::default(), release: Default::default() });
    let log = Arc::new(Log(Mutex::new(vec![entry("w1")])));
    let store = Arc::new(SyncedRecordStore::new(log, gate.clone(), Arc::new(Pinned("2026-11-07T10:00:00.000Z".into())), None).unwrap());
    let flusher = {
        let store = store.clone();
        std::thread::spawn(move || store.flush())
    };
    gate.entered.wait();
    // Returns at once with the flusher parked inside the transport.
    store.flush();
    gate.release.raise();
    flusher.join().unwrap();
    assert!(store.entries().iter().all(|e| !e.is_pending()));
}

#[test]
fn the_overlay_and_the_label_read_the_account_s_entries() {
    let log = Arc::new(Log::default());
    let transport = Arc::new(Scripted::default());
    let store = SyncedRecordStore::new(log, transport, Arc::new(Pinned("2026-11-07T10:00:00.000Z".into())), None).unwrap();
    store.submit(entry("w1"), false).unwrap();
    assert_eq!(store.pending_label("a").as_deref(), Some("1 change waiting to sync"));
    assert_eq!(store.pending_label("b"), None);
    assert_eq!(store.visible_rows("a", Some("me"))[0].author_id.as_deref(), Some("me"));
    assert!(store.rows("a").unwrap().is_empty(), "the store's own rows stay the synced ones");
}

#[test]
fn a_subscriber_hears_what_moved() {
    let store = SyncedRecordStore::new(Arc::new(Log::default()), Arc::new(Scripted::default()), Arc::new(Pinned("2026-11-07T10:00:00.000Z".into())), None).unwrap();
    let heard = Arc::new(Mutex::new(Vec::new()));
    let sink = heard.clone();
    let sub = store.subscribe("a", Box::new(move |d| sink.lock().unwrap().push((d.upserts.len(), d.deletes.len()))));
    let row: eui_core::host::RecordRow =
        serde_json::from_value(json!({"id": "r", "kind": "task", "data": {}, "createdAt": "2026-11-07T09:00:00.000Z", "updatedAt": "2026-11-07T09:00:00.000Z"})).unwrap();
    store.set_rows("a", vec![row.clone()]);
    store.set_rows("a", vec![row]);
    store.set_rows("a", Vec::new());
    drop(sub);
    store.set_rows("a", Vec::new());
    assert_eq!(*heard.lock().unwrap(), vec![(1, 0), (0, 1)]);
}

#[test]
fn a_host_call_becomes_one_entry_with_client_ids() {
    let store = SyncedRecordStore::new(Arc::new(Log::default()), Arc::new(Scripted::default()), Arc::new(Pinned("2026-11-07T10:00:00.000Z".into())), None).unwrap();
    assert_eq!(record_call("records_batch"), Some(OutboxCall::Batch));
    assert_eq!(record_call("exponential_issues_update"), None);
    let args = json!({"writes": [{"kind": "task", "data": {}}, {"kind": "task", "data": {}}, {"op": "delete", "id": "r9"}]});
    let entry = store.entry("a", OutboxCall::Batch, &args);
    assert_eq!((entry.app_id.as_str(), entry.at.as_str(), entry.writes.len()), ("a", "2026-11-07T10:00:00.000Z", 3));
    assert!(entry.is_pending());
    assert_ne!(entry.writes[0].id(), entry.writes[1].id());
    assert_eq!(entry.answer()["ids"][2], "r9");
}
