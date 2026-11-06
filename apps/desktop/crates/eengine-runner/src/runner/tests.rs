use std::sync::Arc;

use eengine::capability::Capability;
use eengine::CallFrom;
use eengine::consent::ConsentAnswer;
use eengine::providers::Failure;
use eengine::{FsBlobStore, MemoryRecordStore, MemoryVault, NetworkPolicy};
use serde_json::{json, Value};

use super::*;

struct Photos;

impl Capability for Photos {
    fn name(&self) -> &str {
        "photos"
    }
    fn tools(&self) -> Vec<Value> {
        vec![]
    }
    fn call(&self, _tool: &str, _arguments: &Value) -> Result<Value, Failure> {
        Ok(json!({"content": [{"type": "text", "text": "{}"}], "structuredContent": {"photos": []}}))
    }
}

fn package() -> Value {
    json!({
        "formatVersion": 5,
        "id": "acme.album",
        "name": "Album",
        "version": "1.0.0",
        "catalogId": "https://ui.exponential.at/catalogs/core/v1",
        "functions": ["records.*", "app.navigate", "photos.pick"],
        "sources": ["records:*"],
        "records": {"note": {"title": "Note", "schema": {"type": "object", "properties": {"text": {"type": "string"}}}}},
        "capabilities": [{"id": "photos.read", "why": "Pick the photos for an album."}],
        "screens": [{"template": "albums", "title": "Albums"}, {"template": "notes", "title": "Notes"}, {"template": "page", "title": "Page", "nav": false}],
        "templates": {
            "albums": {"components": [{"id": "root", "component": "Text", "text": "Albums"}]},
            "notes": {"components": [{"id": "root", "component": "Text", "text": "Notes"}]},
            "page": {"components": [{"id": "root", "component": "Text", "text": "Page"}]}
        }
    })
}

fn config() -> RunnerConfig {
    RunnerConfig { id: "at.acme.album".into(), name: "Album".into(), version: "1.0.0".into(), network: NetworkPolicy::Off }
}

fn providers() -> Providers {
    Providers {
        platform: "linux".into(),
        locale: "en-US".into(),
        storage: Arc::new(MemoryRecordStore::new()),
        blobs: Arc::new(FsBlobStore::new(std::env::temp_dir().join("eengine-runner-test-blobs"))),
        vault: Arc::new(MemoryVault::default()),
        capabilities: vec![Arc::new(Photos)],
    }
}

fn runner(pkg: &Value) -> Runner {
    Runner::boot(config(), pkg, providers(), FormFactor::Wide).expect("boots")
}

fn messages(paints: &[Paint]) -> Vec<&Value> {
    paints.iter().filter_map(|p| if let Paint::Message(m) = p { Some(m) } else { None }).collect()
}

fn surface_of(m: &Value) -> Option<&str> {
    m.as_object()?.values().find_map(|v| v.get("surfaceId")).and_then(Value::as_str)
}

#[test]
fn a_launch_installs_the_shell_and_opens_the_first_screen() {
    let mut r = runner(&package());
    let paints = r.launch(Vec::new());
    let Paint::Install(shell) = &paints[0] else { panic!("the shell first: {paints:?}") };
    let components = shell["templates"]["albums"]["components"].as_array().unwrap();
    assert!(components.iter().any(|c| c["component"] == "Sidebar"), "the wide shell: {components:?}");
    assert!(components.iter().any(|c| c["id"] == "runner-page"));
    let opens = messages(&paints);
    assert_eq!(opens[0], &open_message("acme.album", "albums", &Map::new()));
    assert!(paints.contains(&Paint::Live(true)));
    // The last screen persists: a relaunch opens it.
    r.runner_event(RunnerEvent::Navigate { screen: "notes".into() });
    r.terminate();
    let paints = r.launch(Vec::new());
    assert_eq!(messages(&paints)[0]["applyTemplate"]["templateId"], "notes");
}

#[test]
fn a_package_without_screens_opens_its_first_template() {
    let mut pkg = package();
    pkg.as_object_mut().unwrap().remove("screens");
    pkg["functions"] = json!(["records.*"]);
    pkg.as_object_mut().unwrap().remove("capabilities");
    let mut r = runner(&pkg);
    let paints = r.launch(Vec::new());
    let opens = messages(&paints);
    assert_eq!(opens.len(), 1);
    assert_eq!(opens[0]["applyTemplate"]["templateId"], "albums");
}

#[test]
fn a_package_that_does_not_load_is_a_failure() {
    let mut pkg = package();
    pkg["formatVersion"] = json!("five");
    assert_eq!(Runner::boot(config(), &pkg, providers(), FormFactor::Wide).err(), Some(RunnerFailure::Invalid));
    let signed = json!({"package": package(), "author": {"name": "Acme", "publicKey": "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"}, "signature": "AAAA"});
    assert_eq!(Runner::boot(config(), &signed, providers(), FormFactor::Wide).err(), Some(RunnerFailure::Signature));
    let paints = failure_paints(RunnerFailure::Missing);
    let m = messages(&paints);
    assert_eq!(m[1]["updateComponents"]["components"][0]["component"], "EmptyState");
    assert_eq!(surface_of(m[0]), Some(APP));
}

#[test]
fn navigation_and_a_refused_link() {
    let mut r = runner(&package());
    r.launch(Vec::new());
    let (paints, _) = r.runner_event(RunnerEvent::Navigate { screen: "notes".into() });
    assert_eq!(messages(&paints)[0]["applyTemplate"]["templateId"], "notes");
    let (paints, _) = r.runner_event(RunnerEvent::Navigate { screen: "nowhere".into() });
    assert_eq!(surface_of(messages(&paints)[0]), Some(TOAST));
    // A link to a screen that is not exposed: the lifecycle.json copy.
    let paints = r.lifecycle(&crate::events::deep_link("at.acme.album", "at.acme.album://page").unwrap());
    let toast = messages(&paints);
    assert_eq!(toast[1]["updateComponents"]["components"][0]["title"], "Couldn't open the link");
}

#[test]
fn records_write_and_resolve() {
    let mut r = runner(&package());
    r.launch(Vec::new());
    let Dispatch::Done(result) = r.dispatch("records.upsert", &json!({"kind": "note", "data": {"text": "hi"}})) else { panic!("records answer here") };
    let (answer, _) = r.finish(1, "records.upsert", &Value::Null, result);
    assert!(answer.unwrap().is_ok());
    let value = r.records("records:note").unwrap();
    assert_eq!(value.to_string().matches("\"hi\"").count(), 1, "{value}");
    assert_eq!(new_ids("records.batch", &json!({"writes": [{"kind": "a"}, {"id": "x", "kind": "a"}]})).len(), 1);
}

#[test]
fn an_ask_paints_consent_and_its_answer_settles_the_call() {
    let mut r = runner(&package());
    r.launch(Vec::new());
    let args = json!({});
    let Dispatch::Broker(broker) = r.dispatch("photos.pick", &args) else { panic!("a capability goes to the broker") };
    let (answer, paints) = r.finish(7, "photos.pick", &args, brokered(broker.call(CallFrom::Ui, None, "photos.pick", &args)));
    assert_eq!(answer, None, "it waits for the person");
    let m = messages(&paints);
    assert_eq!(surface_of(m[0]), Some(CONSENT));
    assert_eq!(m[1]["updateComponents"]["components"][0]["component"], "Drawer");
    let (paints, pending) = r.runner_event(RunnerEvent::ConsentAnswer { answer: ConsentAnswer::Allow });
    assert_eq!(messages(&paints)[0], &delete_message(CONSENT));
    let (pending, answer) = pending.expect("the pending call");
    assert_eq!(pending.ticket, 7);
    let result = brokered(broker.answer(CallFrom::Ui, None, &pending.name, &pending.args, &pending.verdict, answer));
    let (answer, _) = r.finish(pending.ticket, &pending.name, &pending.args, result);
    // VAPP-271: genUI gets the structuredContent (an action's `results.picked.photos`).
    assert_eq!(answer.unwrap().unwrap(), json!({"photos": []}));
    assert_eq!(r.instance().grants().get("photos.read"), Some(&GrantValue::Granted));
    assert!(r.pending().is_none());
}

#[test]
fn the_permissions_sheet_toggles_a_grant() {
    let mut r = runner(&package());
    r.launch(Vec::new());
    let (paints, _) = r.runner_event(RunnerEvent::Permissions);
    let m = messages(&paints);
    assert_eq!(surface_of(m[0]), Some(PERMISSIONS));
    assert_eq!(m[1]["updateComponents"]["components"][1]["checked"], false);
    let (paints, _) = r.runner_event(RunnerEvent::ConsentToggle { scope: "photos.read".into() });
    assert_eq!(messages(&paints)[1]["updateComponents"]["components"][1]["checked"], true);
    let (paints, _) = r.runner_event(RunnerEvent::ConsentToggle { scope: "photos.read".into() });
    assert_eq!(messages(&paints)[1]["updateComponents"]["components"][1]["checked"], false);
}

#[test]
fn crossing_md_reinstalls_and_reapplies() {
    let mut r = runner(&package());
    r.launch(Vec::new());
    r.runner_event(RunnerEvent::Navigate { screen: "notes".into() });
    assert!(r.resize(1200.0).is_empty());
    let paints = r.resize(500.0);
    let Paint::Install(shell) = &paints[0] else { panic!() };
    assert_eq!(shell["templates"]["notes"]["components"][0]["component"], "Stack");
    assert_eq!(messages(&paints)[0]["applyTemplate"]["templateId"], "notes");
}

#[test]
fn the_functions_a_package_reaches() {
    let names = function_names(&package());
    for n in ["records.upsert", "records.delete", "records.batch", "photos.get", "photos.list", "photos.pick"] {
        assert!(names.contains(&n.to_string()), "{n}: {names:?}");
    }
}

/// VAPP-271: an `app-file:<fileId>` src reads the instance's BlobStore under
/// its app id, through eui-gpui's media loader (the image limits); a missing
/// file is none (the fallback paints).
#[test]
fn an_app_file_src_reads_the_instances_blob_store() {
    struct Host(Arc<dyn eui_gpui::host::AppFiles>);
    impl eui_gpui::host::HostPlugin for Host {
        fn app_files(&self) -> Option<Arc<dyn eui_gpui::host::AppFiles>> {
            Some(self.0.clone())
        }
    }
    let dir = std::env::temp_dir().join(format!("eengine-runner-app-files-{}", std::process::id()));
    let store = Arc::new(FsBlobStore::new(&dir));
    let r = Runner::boot(config(), &package(), Providers { blobs: store.clone(), ..providers() }, FormFactor::Wide).expect("boots");
    let png = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR\0\0\0\x01\0\0\0\x01\x08\x06\0\0\0\x1f\x15\xc4\x89".to_vec();
    let id = store.put(r.app_id(), "a.png", "image/png", &png).unwrap();
    let files = r.app_files();
    assert_eq!(files.read(&id), Some(png.clone()));
    assert_eq!(files.read("3f2c9a1e-8b7d-4c1e-9f00-1a2b3c4d5e6f"), None);
    let host = Host(files);
    let key = eui_gpui::media::image_key(&host, &format!("app-file:{id}")).expect("an app file loads");
    assert_eq!(eui_gpui::media::fetch(&key).unwrap(), png);
    let gone = eui_gpui::media::image_key(&host, "app-file:3f2c9a1e-8b7d-4c1e-9f00-1a2b3c4d5e6f").unwrap();
    assert!(eui_gpui::media::fetch(&gone).is_err());
    std::fs::remove_dir_all(&dir).ok();
}
