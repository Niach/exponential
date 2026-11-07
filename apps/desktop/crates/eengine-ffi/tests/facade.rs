//! #VAPP-263: the facade from Rust, with providers implemented the way Swift
//! and Kotlin implement them (strings in, strings out): an app's lifecycle,
//! writes through a host store and the default stores, the broker's consent
//! round trip, the outbox, and the host config contract (providers.json).

use std::sync::{Arc, Mutex};

use eengine_ffi::*;
use serde_json::{json, Value};

const PROVIDERS: &str = include_str!("../../../../../packages/eengine/fixtures/providers.json");
const RUNNER: &str = include_str!("../../../../../packages/eengine/fixtures/runner.json");

#[derive(Default)]
struct Keychain(Mutex<std::collections::HashMap<String, String>>);

impl Vault for Keychain {
    fn read(&self, key: String) -> Option<String> {
        self.0.lock().unwrap().get(&key).cloned()
    }

    fn write(&self, key: String, value: String) -> Result<(), EngineError> {
        self.0.lock().unwrap().insert(key, value);
        Ok(())
    }

    fn delete(&self, key: String) -> Result<(), EngineError> {
        self.0.lock().unwrap().remove(&key);
        Ok(())
    }
}

#[derive(Default)]
struct Nav(Mutex<Vec<(String, Value, Presentation)>>);

impl WindowHost for Nav {
    fn present(&self, _app: String, screen: String, params_json: String, how: Presentation) {
        self.0.lock().unwrap().push((screen, serde_json::from_str(&params_json).unwrap(), how));
    }
}

impl Nav {
    fn last(&self) -> String {
        self.0.lock().unwrap().last().unwrap().0.clone()
    }
}

struct Photos;

impl Capability for Photos {
    fn name(&self) -> String {
        "photos".into()
    }

    fn tools_json(&self) -> String {
        "[]".into()
    }

    /// A native result per capabilities.json's `nativeOutputSchema` (the bridge hands it the native arguments).
    fn call(&self, tool: String, arguments_json: String) -> Result<String, EngineError> {
        assert_eq!((tool.as_str(), arguments_json.as_str()), ("pick", r#"{"limit":1}"#));
        Ok(json!({"structuredContent": {"photos": []}}).to_string())
    }
}

/// A host's own store (GRDB, Room) seen through the foreign trait.
#[derive(Default)]
struct HostStore {
    rows: Mutex<Vec<Value>>,
    listeners: Mutex<Vec<(u64, Arc<RecordsListener>)>>,
}

impl RecordStore for HostStore {
    fn rows(&self, _app: String) -> Result<String, EngineError> {
        Ok(Value::Array(self.rows.lock().unwrap().clone()).to_string())
    }

    fn write(&self, _app: String, batch_json: String, by: Option<String>) -> Result<String, EngineError> {
        let batch: Value = serde_json::from_str(&batch_json).unwrap();
        let mut ids = Vec::new();
        for w in batch["writes"].as_array().unwrap() {
            let id = w["id"].as_str().unwrap().to_string();
            let mut rows = self.rows.lock().unwrap();
            match w["op"].as_str().unwrap() {
                "upsert" => rows.push(json!({"id": id, "kind": w["kind"], "data": w["data"], "createdBy": by, "authorId": by, "createdAt": batch["at"], "updatedAt": batch["at"]})),
                _ => rows.retain(|r| r["id"] != json!(id)),
            }
            ids.push(id);
        }
        for (_, listener) in self.listeners.lock().unwrap().iter() {
            listener.changed(json!({"upserts": [], "deletes": []}).to_string());
        }
        Ok(json!({"ids": ids, "replayed": false}).to_string())
    }

    fn history(&self, _app: String, _id: String) -> Result<String, EngineError> {
        Err(EngineError::Failure { code: "NOT_IMPLEMENTED".into(), reason: "no history".into() })
    }

    fn subscribe(&self, _app: String, listener: Arc<RecordsListener>) -> u64 {
        let mut listeners = self.listeners.lock().unwrap();
        listeners.push((7, listener));
        7
    }

    fn unsubscribe(&self, token: u64) {
        self.listeners.lock().unwrap().retain(|(t, _)| *t != token);
    }
}

struct Pinned;

impl EngineClock for Pinned {
    fn now(&self) -> String {
        "2026-11-06T10:00:00.000Z".into()
    }
}

#[derive(Default)]
struct Rows(Mutex<Vec<CallAuditRow>>);

impl CallAudit for Rows {
    fn record(&self, row: CallAuditRow) {
        self.0.lock().unwrap().push(row);
    }
}

struct Online(Mutex<Vec<String>>);

impl SyncTransport for Online {
    fn push(&self, _app: String, call: String, _input_json: String) -> String {
        self.0.lock().unwrap().push(call);
        json!({"ok": true}).to_string()
    }
}

fn package() -> String {
    json!({
        "formatVersion": 5,
        "id": "acme.album",
        "name": "Album",
        "version": "1.0.0",
        "catalogId": "https://ui.exponential.at/catalogs/core/v1",
        "functions": ["records.*", "app_navigate", "photos_pick"],
        "sources": ["records:*"],
        "records": {"note": {"title": "Note", "schema": {"type": "object", "properties": {"text": {"type": "string"}}}, "policy": {"update": "author", "delete": "owners"}}},
        "capabilities": [{"id": "photos.read", "why": "Pick the photos for an album."}],
        "screens": [{"template": "albums", "title": "Albums"}, {"template": "page", "title": "Page", "nav": false}],
        "templates": {
            "albums": {"components": [{"id": "root", "component": "Text", "text": "Albums"}]},
            "page": {"components": [{"id": "root", "component": "Text", "text": "Page"}]}
        }
    })
    .to_string()
}

fn host(data_dir: Option<String>) -> EngineHost {
    EngineHost { kind: "exported".into(), platform: "ios".into(), form_factor: "phone".into(), locale: "en".into(), zone: None, network: NetworkPolicy::Off, data_dir }
}

fn providers(vault: Arc<Keychain>, nav: Arc<Nav>) -> EngineProviders {
    EngineProviders {
        vault,
        window: nav,
        storage: None,
        blobs: None,
        sync: None,
        capabilities: vec![Arc::new(Photos)],
        consent: None,
        audit: None,
        clock: Some(Arc::new(Pinned)),
    }
}

fn local() -> EngineIdentity {
    EngineIdentity::Local { device_id: "d-1".into() }
}

fn value(result: CallResult) -> Value {
    match result {
        CallResult::Value { json } => serde_json::from_str(&json).unwrap(),
        other => panic!("{other:?}"),
    }
}

#[test]
fn an_app_runs_over_a_host_store() {
    let nav = Arc::new(Nav::default());
    let store = Arc::new(HostStore::default());
    let mut p = providers(Arc::new(Keychain::default()), nav.clone());
    p.storage = Some(store.clone());
    let engine = Engine::new(host(None), local(), p).unwrap();
    let app = engine.load(package()).unwrap();
    assert_eq!(app.app_id(), "acme.album");
    assert_eq!(app.signer(), None);
    assert!(!app.queues());

    let effects: Value = serde_json::from_str(&app.event(r#"{"type":"launch"}"#.into()).unwrap()).unwrap();
    assert!(effects.as_array().unwrap().contains(&json!({"effect": "flush"})));
    assert_eq!(nav.last(), "albums");
    assert_eq!(nav.0.lock().unwrap()[0].2, Presentation::Replace);

    assert_eq!(value(app.call("records_upsert".into(), json!({"kind": "note", "data": {"text": "hi"}}).to_string(), vec!["n1".into()]).unwrap()), json!({"id": "n1"}));
    assert_eq!(store.rows.lock().unwrap()[0]["authorId"], json!("d-1"), "local identity = me");
    let count: Value = serde_json::from_str(&app.records_value("records:note".into()).unwrap()).unwrap();
    assert_eq!(count["count"], json!(1));
    assert_eq!(app.records_value("screen:albums".into()), None, "not a records source");

    assert_eq!(value(app.call("app_navigate".into(), json!({"screen": "page"}).to_string(), vec![]).unwrap()), Value::Null);
    assert_eq!(nav.last(), "page");
    assert!(matches!(app.event("{\"type\":\"nope\"}".into()), Err(EngineError::Invalid { .. })));
    assert!(matches!(app.call("records_upsert".into(), "{".into(), vec![]), Err(EngineError::Invalid { .. })));
}

#[test]
fn the_consent_round_trip_crosses_the_facade() {
    let vault = Arc::new(Keychain::default());
    let audit = Arc::new(Rows::default());
    let mut p = providers(vault.clone(), Arc::new(Nav::default()));
    p.audit = Some(audit.clone());
    let app = Engine::new(host(None), local(), p).unwrap().load(package()).unwrap();
    app.event(r#"{"type":"launch"}"#.into()).unwrap();
    let (verdict, sheet) = match app.call("photos_pick".into(), "{}".into(), vec![]).unwrap() {
        CallResult::Ask { verdict_json, sheet } => (verdict_json, sheet),
        other => panic!("{other:?}"),
    };
    assert_eq!(sheet.scope, "photos.read");
    let components: Value = serde_json::from_str(&sheet.components_json).unwrap();
    assert_eq!(components[0]["title"], json!("Allow Album to access your photos?"));
    let picked = value(app.answer_consent("photos_pick".into(), "{}".into(), verdict, ConsentAnswer::Allow).unwrap());
    assert_eq!(picked, json!({"photos": []}), "VAPP-271: app UI reads the structuredContent");
    assert_eq!(app.grants_json(), r#"{"photos.read":"granted"}"#);
    assert!(vault.read("eengine/grants/acme.album".into()).unwrap().contains("granted"));
    assert_eq!(audit.0.lock().unwrap().last().unwrap().tool, "photos__pick");

    app.set_grant("photos.read".into(), Some(GrantValue::Denied)).unwrap();
    let sheet: Value = serde_json::from_str(&app.permissions_sheet_json()).unwrap();
    assert_eq!(sheet[1]["checked"], json!(false));
    match app.call("camera_capture".into(), "{}".into(), vec![]).unwrap() {
        CallResult::Refused { message, .. } => assert_eq!(message, "notDeclared"),
        other => panic!("{other:?}"),
    }
    assert!(matches!(app.set_grant("camera_capture".into(), Some(GrantValue::Granted)), Err(EngineError::Failure { .. })));
}

struct AlwaysAllow;

impl ConsentUi for AlwaysAllow {
    fn ask(&self, app: String, sheet: ConsentSheet) -> ConsentAnswer {
        assert_eq!((app.as_str(), sheet.scope.as_str()), ("Album", "photos.read"));
        ConsentAnswer::Once
    }
}

#[test]
fn an_inline_consent_ui_answers_the_sheet() {
    let mut p = providers(Arc::new(Keychain::default()), Arc::new(Nav::default()));
    p.consent = Some(Arc::new(AlwaysAllow));
    let app = Engine::new(host(None), local(), p).unwrap().load(package()).unwrap();
    app.event(r#"{"type":"launch"}"#.into()).unwrap();
    assert_eq!(value(app.call("photos_pick".into(), "{}".into(), vec![]).unwrap()), json!({"photos": []}), "VAPP-271: app UI reads the structuredContent");
    assert_eq!(app.grants_json(), "{}", "once stores nothing");
}

#[test]
fn a_synced_host_queues_writes() {
    let sync = Arc::new(Online(Mutex::new(Vec::new())));
    let mut p = providers(Arc::new(Keychain::default()), Arc::new(Nav::default()));
    p.sync = Some(sync.clone());
    let app = Engine::new(host(None), EngineIdentity::Member { user_id: "u1".into(), role: Some("member".into()), access: None }, p).unwrap().load(package()).unwrap();
    assert!(app.queues());
    app.event(r#"{"type":"launch"}"#.into()).unwrap();
    app.call("records_upsert".into(), json!({"kind": "note", "data": {"text": "hi"}}).to_string(), vec!["n1".into()]).unwrap();
    assert_eq!(*sync.0.lock().unwrap(), vec!["upsert".to_string()]);
    assert_eq!(app.pending(), 0, "acked");
    let count: Value = serde_json::from_str(&app.records_value("records:note".into()).unwrap()).unwrap();
    assert_eq!(count["count"], json!(1), "acked entries overlay until the store reflects them");
}

#[test]
fn packages_that_do_not_load_say_why() {
    let engine = Engine::new(host(None), local(), providers(Arc::new(Keychain::default()), Arc::new(Nav::default()))).unwrap();
    let mut pkg: Value = serde_json::from_str(&package()).unwrap();
    pkg["formatVersion"] = json!(4);
    match engine.clone().load(pkg.to_string()) {
        Err(EngineError::Load { kind, issues_json }) => {
            assert_eq!(kind, "invalid");
            assert_eq!(serde_json::from_str::<Value>(&issues_json).unwrap()[0]["path"], json!("/formatVersion"));
        }
        Err(other) => panic!("{other:?}"),
        Ok(_) => panic!("loaded a format 4 package"),
    }
    assert!(matches!(engine.load("not json".into()), Err(EngineError::Invalid { .. })));
}

#[test]
fn capabilities_must_be_catalog_names_once() {
    let mut p = providers(Arc::new(Keychain::default()), Arc::new(Nav::default()));
    p.capabilities = vec![Arc::new(Photos), Arc::new(Photos)];
    assert!(matches!(Engine::new(host(None), local(), p), Err(EngineError::Invalid { .. })));
}

#[cfg(feature = "sqlite")]
#[test]
fn the_default_store_is_sqlite_under_the_data_dir() {
    let dir = std::env::temp_dir().join(format!("eengine-ffi-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let dir_text = dir.to_string_lossy().to_string();
    for round in 0..2 {
        let app = Engine::new(host(Some(dir_text.clone())), local(), providers(Arc::new(Keychain::default()), Arc::new(Nav::default()))).unwrap().load(package()).unwrap();
        app.event(r#"{"type":"launch"}"#.into()).unwrap();
        if round == 0 {
            app.call("records_upsert".into(), json!({"kind": "note", "data": {"text": "kept"}}).to_string(), vec!["n1".into()]).unwrap();
        }
        let count: Value = serde_json::from_str(&app.records_value("records:note".into()).unwrap()).unwrap();
        assert_eq!(count["count"], json!(1), "round {round}: the record survives a new Engine");
    }
    assert!(dir.join("records.sqlite").exists());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn host_configs_validate_like_the_fixture() {
    let fixture: Value = serde_json::from_str(PROVIDERS).unwrap();
    for case in fixture["validateCases"].as_array().unwrap() {
        let got: Value = serde_json::from_str(&validate_host_config(case["config"].to_string()).unwrap()).unwrap();
        assert_eq!(got, case["expected"], "{}", case["name"]);
    }
    assert!(validate_host_config("[]".into()).is_err());
    assert_eq!(engine_version(), env!("CARGO_PKG_VERSION"));
    assert_eq!(engine_features().contains(&"sqlite".to_string()), cfg!(feature = "sqlite"));
}

#[test]
fn the_runner_contract_crosses_the_facade() {
    let fixture: Value = serde_json::from_str(RUNNER).unwrap();
    for case in fixture["configCases"].as_array().unwrap() {
        let got: Value = serde_json::from_str(&runner_config(case["input"].to_string()).unwrap()).unwrap();
        assert_eq!(got, case["expected"], "{}", case["name"]);
    }
    for case in fixture["shellCases"].as_array().unwrap() {
        let form = if case["formFactor"] == "wide" { RunnerFormFactor::Wide } else { RunnerFormFactor::Compact };
        let got: Value = serde_json::from_str(&runner_package(case["package"].to_string(), form).unwrap()).unwrap();
        assert_eq!(got, case["expected"], "{}", case["name"]);
    }
    for case in fixture["failureCases"].as_array().unwrap() {
        let got: Value = serde_json::from_str(&runner_failure(case["kind"].as_str().unwrap().into()).unwrap()).unwrap();
        assert_eq!(got, case["expected"]);
    }
    assert!(runner_failure("nope".into()).is_err());
    let toast = &fixture["toastCases"][0];
    assert_eq!(serde_json::from_str::<Value>(&runner_toast(toast["message"].as_str().unwrap().into())).unwrap(), toast["expected"]);
}

#[cfg(feature = "hub")]
#[test]
fn an_agent_reaches_the_capabilities_through_the_hub() {
    let app = Engine::new(host(None), local(), providers(Arc::new(Keychain::default()), Arc::new(Nav::default()))).unwrap().load(package()).unwrap();
    let access = app.agent_access("run-1".into()).unwrap();
    assert!(access.url.starts_with("http://127.0.0.1:"), "{}", access.url);
    assert!(!access.token.is_empty());
    assert_eq!(access.tools, vec!["photos__get".to_string(), "photos__list".to_string(), "photos__pick".to_string()], "the catalog's tools of photos.read");
    let again = app.agent_access("run-1".into()).unwrap();
    assert_eq!(again.url, access.url, "one hub per Engine");
    assert_ne!(again.token, access.token, "a new token replaces the last");
    app.revoke_agent("run-1".into());
    assert!(app.agent_access("not a session id!".into()).is_err());
}

// --- The synced store (VAPP-272) ------------------------------------------

#[derive(Default)]
struct RoomLog(Mutex<Vec<String>>);

impl eengine_ffi::OutboxLog for RoomLog {
    fn entries_json(&self) -> Result<String, EngineError> {
        Ok(format!("[{}]", self.0.lock().unwrap().join(",")))
    }
    fn push(&self, entry_json: String) -> Result<(), EngineError> {
        self.0.lock().unwrap().push(entry_json);
        Ok(())
    }
    fn ack(&self, _write_id: String, _at: String) -> Result<(), EngineError> {
        Ok(())
    }
    fn remove(&self, write_ids: Vec<String>) -> Result<(), EngineError> {
        self.0.lock().unwrap().retain(|e| !write_ids.iter().any(|id| e.contains(id.as_str())));
        Ok(())
    }
}

struct Answers(&'static str);

impl eengine_ffi::SyncTransport for Answers {
    fn push(&self, _app: String, _call: String, _input_json: String) -> String {
        self.0.to_string()
    }
}

const CREATE: &str = r#"{"kind":"task","data":{"title":"t"}}"#;

fn synced(log: Arc<RoomLog>, reply: &'static str) -> Arc<eengine_ffi::SyncedRecordStore> {
    eengine_ffi::SyncedRecordStore::new(log, Arc::new(Answers(reply)), None, None).unwrap()
}

#[test]
fn the_synced_store_answers_the_server_s_own_answer() {
    let store = synced(Arc::default(), r#"{"ok":true,"answer":{"id":"r1"}}"#);
    let got = store.submit_call("a".into(), "records_upsert".into(), CREATE.into(), true).unwrap();
    assert_eq!(got, eengine_ffi::Submitted::Done { answer_json: r#"{"id":"r1"}"#.into() });
    assert_eq!(store.pending_label("a".into()), None);
}

#[test]
fn the_synced_store_carries_a_refusal_and_queues_offline() {
    let log = Arc::new(RoomLog::default());
    let store = synced(log.clone(), r#"{"code":"BAD_REQUEST","message":"title","invalid":true}"#);
    let got = store.submit_call("a".into(), "records_upsert".into(), CREATE.into(), true).unwrap();
    assert_eq!(got, eengine_ffi::Submitted::Refused { message: Some("title".into()), invalid: true });
    assert!(log.0.lock().unwrap().is_empty(), "the refused entry leaves the log");
    let eengine_ffi::Submitted::Queued { answer_json } = store.submit_call("a".into(), "records_upsert".into(), CREATE.into(), false).unwrap() else { panic!("queued") };
    let id = serde_json::from_str::<Value>(&answer_json).unwrap()["id"].as_str().unwrap().to_string();
    assert_eq!(store.pending_label("a".into()).as_deref(), Some("1 change waiting to sync"));
    assert!(store.visible_rows_json("a".into(), Some("me".into())).contains(&id));
    assert!(store.submit_call("a".into(), "exponential_issues_update".into(), "{}".into(), false).is_err());
}

#[test]
fn one_unreadable_row_does_not_stop_the_queue() {
    let log = Arc::new(RoomLog::default());
    log.0.lock().unwrap().push(r#"{"writeId":"bad","appId":"a"}"#.to_string());
    log.0.lock().unwrap().push(r#"{"writeId":"w1","appId":"a","call":"delete","writes":[{"op":"delete","id":"r1"}],"at":"2026-11-07T09:00:00.000Z","state":"pending"}"#.to_string());
    let store = synced(log, r#"{"ok":true}"#);
    assert_eq!(store.pending_label("a".into()).as_deref(), Some("1 change waiting to sync"));
}
