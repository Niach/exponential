//! VAPP-261: an AppInstance over local providers, and package verification
//! against genUI's signature fixture.

use std::sync::{Arc, Mutex};

use eengine::broker::GrantValue;
use eengine::capability::{CallAudit, Capability};
use eengine::consent::{ConsentAnswer, GrantVia};
use eengine::instance::{screen_key, AppInstance, CallResult, LoadError};
use eengine::lifecycle::{Effect, LifecycleEvent};
use eengine::outbox::OutboxResult;
use eengine::package::{verify_package_file, Verification};
use eengine::providers::{Clock, Failure, HostConfig, HostInfo, NetworkPolicy, Presentation, RecordStore, SyncTransport, Vault, WindowHost};
use eengine::stores::{FsBlobStore, LocalIdentity, MemoryRecordStore, MemoryVault};
use serde_json::{json, Map, Value};

const HOST_PACKAGE: &str = include_str!("../../../../../packages/exponential-ui/fixtures/host-package.json");

#[test]
fn signatures_verify_like_the_ts_reference() {
    let fixture: Value = serde_json::from_str(HOST_PACKAGE).unwrap();
    for c in fixture["signatures"].as_array().unwrap() {
        let got = verify_package_file(&c["signed"]);
        assert_eq!(serde_json::to_value(&got).unwrap(), c["expected"], "{}", c["name"]);
    }
}

#[derive(Default)]
struct Windows(Mutex<Vec<(String, Map<String, Value>)>>);

impl WindowHost for Windows {
    fn open(&self, _app: &str, screen: &str, params: &Map<String, Value>, _how: Presentation) {
        self.0.lock().unwrap().push((screen.to_string(), params.clone()));
    }
}

struct FixedClock;

impl Clock for FixedClock {
    fn now(&self) -> String {
        "2026-11-06T10:00:00.000Z".into()
    }
}

fn package() -> Value {
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
}

fn config(store: Arc<dyn RecordStore>, vault: Arc<dyn Vault>, windows: Arc<Windows>, sync: Option<Arc<dyn SyncTransport>>) -> HostConfig {
    HostConfig {
        host: HostInfo { kind: "exported".into(), platform: "linux".into(), form_factor: "desktop".into(), locale: "en".into() },
        storage: store,
        blobs: Arc::new(FsBlobStore::new(std::env::temp_dir().join("eengine-test-blobs"))),
        identity: Arc::new(LocalIdentity { device_id: "d-1".into() }),
        vault,
        window: windows,
        clock: Arc::new(FixedClock),
        zone: None,
        network: NetworkPolicy::Off,
        sync,
        capabilities: vec![Arc::new(Photos)],
        consent: None,
        audit: Some(Arc::new(Audit::default())),
    }
}

#[test]
fn a_local_instance_runs_its_lifecycle_and_writes_through() {
    let store = Arc::new(MemoryRecordStore::new());
    let vault: Arc<dyn Vault> = Arc::new(MemoryVault::default());
    let windows = Arc::new(Windows::default());
    let mut app = AppInstance::load(config(store.clone(), vault.clone(), windows.clone(), None), &package()).expect("loads");
    let effects = app.event(&LifecycleEvent::Launch);
    assert!(effects.contains(&Effect::Flush));
    assert_eq!(windows.0.lock().unwrap().last().unwrap().0, "albums");

    let created = app.call("records_upsert", &json!({"kind": "note", "data": {"text": "hi"}}), &["n1".to_string()]);
    assert_eq!(created, CallResult::Value(json!({"id": "n1"})));
    let rows = store.rows("acme.album").unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].author_id.as_deref(), Some("d-1"), "local identity = me");
    assert_eq!(app.records_value("records:note").unwrap()["count"], json!(1));
    // Local identity collapses `owners` to me.
    assert_eq!(app.call("records_delete", &json!({"id": "n1"}), &[]), CallResult::Value(Value::Null));
    assert!(store.rows("acme.album").unwrap().is_empty());

    // app.navigate opens a declared screen; terminate persists it; a relaunch restores it.
    assert_eq!(app.call("app_navigate", &json!({"screen": "page"}), &[]), CallResult::Value(Value::Null));
    app.event(&LifecycleEvent::Terminate);
    assert!(vault.read(&screen_key("acme.album")).unwrap().contains("page"));
    app.event(&LifecycleEvent::Launch);
    assert_eq!(windows.0.lock().unwrap().last().unwrap().0, "page");
}

struct Photos;

impl Capability for Photos {
    fn name(&self) -> &str {
        "photos"
    }

    fn tools(&self) -> Vec<Value> {
        vec![]
    }

    /// Native results per capabilities.json's `nativeOutputSchema`.
    fn call(&self, tool: &str, arguments: &Value) -> Result<Value, Failure> {
        assert_eq!((tool, arguments), ("pick", &json!({"limit": 1})), "the bridge fills the default");
        Ok(json!({"structuredContent": {"photos": []}}))
    }
}

#[derive(Default)]
struct Audit(Mutex<Vec<eengine::broker::BrokerAuditRow>>);

impl CallAudit for Audit {
    fn record(&self, row: eengine::broker::BrokerAuditRow) {
        self.0.lock().unwrap().push(row);
    }
}

#[test]
fn capability_calls_go_through_the_broker() {
    let windows = Arc::new(Windows::default());
    let vault: Arc<dyn Vault> = Arc::new(MemoryVault::default());
    let mut app = AppInstance::load(config(Arc::new(MemoryRecordStore::new()), vault.clone(), windows, None), &package()).unwrap();
    app.event(&LifecycleEvent::Launch);
    let (verdict, sheet) = match app.call("photos_pick", &json!({}), &[]) {
        CallResult::Ask(v, sheet) => (v, sheet),
        other => panic!("{other:?}"),
    };
    assert_eq!(sheet.components[0]["title"], json!("Allow Album to access your photos?"));
    match app.answer_consent("photos_pick", &json!({}), &verdict, ConsentAnswer::Allow) {
        // VAPP-271: app UI reads the structuredContent.
        CallResult::Value(v) => assert_eq!(v, json!({"photos": []})),
        other => panic!("{other:?}"),
    }
    assert!(vault.read("eengine/grants/acme.album").unwrap().contains("\"photos.read\":\"granted\""));
    assert!(matches!(app.call("photos_pick", &json!({}), &[]), CallResult::Value(_)), "granted: no second sheet");
    match app.call("photos_pick", &json!({"limit": 0}), &[]) {
        // An isError result is a refusal.
        CallResult::Refused(f) => assert_eq!((f.code.as_str(), f.message.as_str()), ("failed", "invalidParams: /limit minimum")),
        other => panic!("{other:?}"),
    }
    // Only the ConsentUi after a gesture changes a grant.
    assert_eq!(app.set_grant(GrantVia::Agent, true, "photos.read", None), Err("onlyConsentUi"));
    assert_eq!(app.set_grant(GrantVia::ConsentUi, false, "photos.read", None), Err("noGesture"));
    assert_eq!(app.set_grant(GrantVia::ConsentUi, true, "photos.read", None), Ok(()));
    assert!(matches!(app.call("photos_pick", &json!({}), &[]), CallResult::Ask(..)), "revoked: asks again");
    assert_eq!(app.set_grant(GrantVia::ConsentUi, true, "photos.read", Some(GrantValue::Denied)), Ok(()));
    assert_eq!(app.permissions_sheet()[1]["checked"], json!(false));
    match app.call("camera_capture", &json!({}), &[]) {
        CallResult::Refused(f) => assert_eq!(f.message, "notDeclared"),
        other => panic!("{other:?}"),
    }
}

struct Online(Mutex<Vec<String>>);

impl SyncTransport for Online {
    fn push(&self, _app: &str, call: &str, _input: &Value) -> OutboxResult {
        self.0.lock().unwrap().push(call.to_string());
        OutboxResult::Ok
    }
}

#[test]
fn a_synced_instance_queues_and_flushes() {
    let sync = Arc::new(Online(Mutex::new(Vec::new())));
    let windows = Arc::new(Windows::default());
    let mut app = AppInstance::load(config(Arc::new(MemoryRecordStore::new()), Arc::new(MemoryVault::default()), windows, Some(sync.clone())), &package()).unwrap();
    app.event(&LifecycleEvent::Launch);
    app.call("records_upsert", &json!({"kind": "note", "data": {"text": "hi"}}), &["n1".to_string()]);
    assert_eq!(*sync.0.lock().unwrap(), vec!["upsert".to_string()]);
    // Acked, not yet reflected by the store: it keeps overlaying.
    assert_eq!(app.records_value("records:note").unwrap()["count"], json!(1));
}

#[test]
fn an_invalid_package_does_not_load() {
    let mut pkg = package();
    pkg["formatVersion"] = json!(4);
    let windows = Arc::new(Windows::default());
    match AppInstance::load(config(Arc::new(MemoryRecordStore::new()), Arc::new(MemoryVault::default()), windows, None), &pkg) {
        Err(LoadError::Invalid(issues)) => assert_eq!(issues[0].path, "/formatVersion"),
        Err(other) => panic!("{other:?}"),
        Ok(_) => panic!("loaded a format 4 package"),
    }
}

#[cfg(feature = "sqlite")]
#[test]
fn sqlite_dedupes_write_ids_and_keeps_history() {
    use eengine::providers::{RecordWrite, WriteBatch};
    let store = eengine::sqlite::SqliteRecordStore::in_memory().unwrap();
    let batch = WriteBatch { write_id: "w1".into(), writes: vec![RecordWrite::Upsert { id: "r1".into(), kind: "note".into(), data: Map::new(), create: true }], at: "2026-11-06T10:00:00.000Z".into() };
    let first = store.write("a", &batch, Some("d-1")).unwrap();
    let again = store.write("a", &batch, Some("d-1")).unwrap();
    assert!(!first.replayed && again.replayed);
    assert_eq!(again.ids, vec!["r1".to_string()]);
    assert_eq!(store.rows("a").unwrap().len(), 1);
    let update = WriteBatch { write_id: "w2".into(), writes: vec![RecordWrite::Upsert { id: "r1".into(), kind: "note".into(), data: json!({"text": "x"}).as_object().unwrap().clone(), create: false }], at: "2026-11-06T10:01:00.000Z".into() };
    store.write("a", &update, Some("d-1")).unwrap();
    let history = store.history("a", "r1").unwrap();
    assert_eq!(history.iter().map(|e| e.op.as_str()).collect::<Vec<_>>(), vec!["create", "update"]);
    assert_eq!(store.rows("a").unwrap()[0].data["text"], json!("x"));
    let missing = WriteBatch { write_id: "w3".into(), writes: vec![RecordWrite::Delete { id: "nope".into() }], at: "2026-11-06T10:02:00.000Z".into() };
    assert_eq!(store.write("a", &missing, None).unwrap_err().code, "NOT_FOUND");
}
