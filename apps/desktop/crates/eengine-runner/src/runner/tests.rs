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
        "functions": ["records.*", "app_navigate", "photos_pick"],
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
        host_kind: "exported".into(),
        platform: "linux".into(),
        locale: "en-US".into(),
        storage: Arc::new(MemoryRecordStore::new()),
        blobs: Arc::new(FsBlobStore::new(std::env::temp_dir().join("eengine-runner-test-blobs"))),
        vault: Arc::new(MemoryVault::default()),
        capabilities: vec![Arc::new(Photos)],
        identity: None,
        synced: None,
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
    assert!(components.iter().any(|c| c["id"] == "runner_page"));
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
    let Dispatch::Done(result) = r.dispatch("records_upsert", &json!({"kind": "note", "data": {"text": "hi"}})) else { panic!("records answer here") };
    let (answer, _) = r.finish(1, "records_upsert", &Value::Null, result);
    assert!(answer.unwrap().is_ok());
    let value = r.records("records:note").unwrap();
    assert_eq!(value.to_string().matches("\"hi\"").count(), 1, "{value}");
    assert_eq!(new_ids("records_batch", &json!({"writes": [{"kind": "a"}, {"id": "x", "kind": "a"}]})).len(), 1);
}

#[test]
fn an_ask_paints_consent_and_its_answer_settles_the_call() {
    let mut r = runner(&package());
    r.launch(Vec::new());
    let args = json!({});
    let Dispatch::Broker(broker) = r.dispatch("photos_pick", &args) else { panic!("a capability goes to the broker") };
    let (answer, paints) = r.finish(7, "photos_pick", &args, brokered(broker.call(CallFrom::Ui, None, "photos_pick", &args)));
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
fn a_surface_painted_again_gets_a_minted_id() {
    let mut s = LiveSurfaces::default();
    let [create, update] = &surface_messages(TOAST, &[json!({"id": "root", "component": "Text", "text": "x"})], None)[..] else { panic!() };
    assert_eq!(create["version"], eui_core::catalog::A2UI_VERSION);
    let never = |_: &str| -> String { panic!("a first paint keeps its id") };
    assert_eq!(s.rewrite(create, never), (create.clone(), None));
    let (again, delete) = s.rewrite(create, |base| format!("{base}#1"));
    assert_eq!(again["createSurface"]["surfaceId"], "toast#1");
    assert_eq!(delete, Some(delete_message("toast")));
    assert_eq!(s.rewrite(update, never).0["updateComponents"]["surfaceId"], "toast#1");
    assert_eq!(s.live(TOAST), "toast#1");
    assert_eq!(s.rewrite(&delete_message(TOAST), never).0, delete_message("toast#1"));
    assert_eq!(s.live(TOAST), TOAST);
    let (third, delete) = s.rewrite(create, |base| format!("{base}#2"));
    assert_eq!((third["createSurface"]["surfaceId"].as_str(), delete), (Some("toast#2"), None));
}

#[test]
fn the_functions_a_package_reaches() {
    let names = function_names(&package());
    for n in ["records_upsert", "records_delete", "records_batch", "photos_get", "photos_list", "photos_pick"] {
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

/// VAPP-271: the signed Photobook sample end to end on the runner, headless:
/// what its `addPage` and `exportPdf` actions call, in order, through the
/// real desktop `photos` capability (the picker = a fake `Dialogs`, the one
/// seam a real NSOpenPanel sits behind) and the Engine's `print.pdf`.
/// `photos.read` asks first and Allow settles it; the app reads each answer's
/// `structuredContent`. The picked photos land in the BlobStore as fileIds,
/// and the PDF too.
#[test]
fn the_photobook_sample_picks_photos_and_exports_a_pdf() {
    use eengine_desktop::{Dialogs, NamedBytes, NativeResult, OpenRequest, PhotosCapability, PrintCapability, Printer, SaveRequest};
    use std::path::PathBuf;

    struct Picker(Vec<PathBuf>);
    impl Dialogs for Picker {
        fn open(&self, _request: &OpenRequest) -> NativeResult<Option<Vec<PathBuf>>> {
            Ok(Some(self.0.clone()))
        }
        fn save(&self, _request: &SaveRequest) -> NativeResult<Option<PathBuf>> {
            Ok(None)
        }
    }
    /// print.pdf is the Engine's own layout: the print sheet is never reached.
    struct NoPrinter;
    impl Printer for NoPrinter {
        fn print(&self, _job: &NamedBytes) -> NativeResult<bool> {
            Ok(false)
        }
    }
    fn b64(text: &str) -> Vec<u8> {
        let alphabet = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let (mut out, mut acc, mut bits) = (Vec::new(), 0u32, 0);
        for c in text.bytes().filter(|c| *c != b'=') {
            acc = (acc << 6) | alphabet.iter().position(|a| *a == c).expect("base64") as u32;
            bits += 6;
            if bits >= 8 {
                bits -= 8;
                out.push((acc >> bits) as u8);
            }
        }
        out
    }
    /// A capability call (an Ask answered Allow): its value, and whether it asked.
    fn allowed(r: &mut Runner, ticket: u64, name: &str, args: &Value) -> (Value, bool) {
        let Dispatch::Broker(broker) = r.dispatch(name, args) else { panic!("{name} goes to the broker") };
        let (answer, paints) = r.finish(ticket, name, args, brokered(broker.call(CallFrom::Ui, None, name, args)));
        if let Some(answer) = answer {
            return (answer.unwrap_or_else(|e| panic!("{name}: {e}")), false);
        }
        assert_eq!(surface_of(messages(&paints)[0]), Some(CONSENT));
        let (_, pending) = r.runner_event(RunnerEvent::ConsentAnswer { answer: ConsentAnswer::Allow });
        let (pending, answer) = pending.expect("the pending call");
        let result = brokered(broker.answer(CallFrom::Ui, None, &pending.name, &pending.args, &pending.verdict, answer));
        let (answer, _) = r.finish(pending.ticket, &pending.name, &pending.args, result);
        (answer.expect("settled").unwrap_or_else(|e| panic!("{name}: {e}")), true)
    }

    let file: Value = serde_json::from_slice(include_bytes!("../../../../../../samples/apps/photobook.app.json")).unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let blobs: Arc<FsBlobStore> = Arc::new(FsBlobStore::new(tmp.path().join("files")));
    let jpg = b64(eengine::contract::CAPABILITIES["pdfCases"][3]["blobs"]["jpg1"].as_str().unwrap());
    let photos: Vec<PathBuf> = ["lake.jpg", "jetty.jpg"].iter().map(|n| tmp.path().join(n)).collect();
    for p in &photos {
        std::fs::write(p, &jpg).unwrap();
    }
    let providers = Providers {
        platform: "macos".into(),
        blobs: blobs.clone(),
        capabilities: vec![Arc::new(PhotosCapability::new(None, Some(Arc::new(Picker(photos))))), Arc::new(PrintCapability::new(Arc::new(NoPrinter)))],
        ..providers()
    };
    let config = RunnerConfig { id: "at.exponential.samples.photobook".into(), name: "Photobook".into(), version: "1.0.0".into(), network: NetworkPolicy::Off };
    let mut r = Runner::boot(config, &file, providers, FormFactor::Wide).expect("the signed sample boots");
    let paints = r.launch(Vec::new());
    assert_eq!(messages(&paints)[0]["applyTemplate"]["templateId"], "albums");

    // newAlbum
    let Dispatch::Done(result) = r.dispatch("records_upsert", &json!({"kind": "album", "data": {"title": "Summer 2026"}})) else { panic!() };
    let album = r.finish(1, "records_upsert", &Value::Null, result).0.unwrap().unwrap();
    let album = album["record"]["id"].as_str().or_else(|| album["id"].as_str()).map(str::to_string).unwrap_or_else(|| panic!("an album id: {album}"));

    // addPage: photos.pick {limit: 4} → fileIds in the BlobStore → the page.
    let (picked, asked) = allowed(&mut r, 2, "photos_pick", &json!({"limit": 4}));
    assert!(asked, "photos.read asks");
    let ids: Vec<String> = picked["photos"].as_array().unwrap_or_else(|| panic!("picked: {picked}")).iter().map(|p| p["fileId"].as_str().unwrap_or_else(|| panic!("a fileId: {picked}")).to_string()).collect();
    assert_eq!(ids.len(), 2);
    for id in &ids {
        assert_eq!(blobs.get(r.app_id(), id).unwrap(), Some(jpg.clone()));
    }
    let Dispatch::Done(result) = r.dispatch("records_upsert", &json!({"kind": "page", "data": {"album": album, "photos": ids, "caption": "At the lake"}})) else { panic!() };
    r.finish(3, "records_upsert", &Value::Null, result).0.unwrap().unwrap();
    let pages = r.records(&format!("records:page?album={album}")).unwrap();
    assert_eq!(pages["count"], 1, "{pages}");

    // exportPdf: print.pdf → a PDF in the BlobStore.
    let blocks = json!([{"heading": "Summer 2026"}, {"image": ids[0]}, {"image": ids[1]}, {"text": "At the lake"}, {"pageBreak": true}]);
    let (pdf, _) = allowed(&mut r, 4, "print_pdf", &json!({"title": "Summer 2026", "blocks": blocks}));
    let pdf_id = pdf["file"]["fileId"].as_str().unwrap_or_else(|| panic!("a PDF fileId: {pdf}"));
    assert_eq!(pdf["file"]["name"], "Summer 2026.pdf");
    let bytes = blobs.get(r.app_id(), pdf_id).unwrap().expect("the PDF in the BlobStore");
    assert!(bytes.starts_with(b"%PDF-"), "a PDF");
    assert_eq!(r.instance().grants().get("photos.read"), Some(&GrantValue::Granted));
}

// #VAPP-273: a connected host's synced store behind the runner.
mod synced {
    use super::*;
    use std::sync::Mutex;

    use eengine::outbox::{OutboxEntry, OutboxResult};
    use eengine::providers::{EngineIdentity, Identity, Pushed, SyncTransport, SystemClock};
    use eengine::{OutboxLog, SyncedRecordStore};
    use eui_core::host::RecordRow;

    #[derive(Default)]
    struct MemoryLog(Mutex<Vec<OutboxEntry>>);

    impl OutboxLog for MemoryLog {
        fn entries(&self) -> Result<Vec<OutboxEntry>, Failure> {
            Ok(self.0.lock().unwrap().clone())
        }
        fn push(&self, entry: &OutboxEntry) -> Result<(), Failure> {
            self.0.lock().unwrap().push(entry.clone());
            Ok(())
        }
        fn ack(&self, write_id: &str, at: &str) -> Result<(), Failure> {
            for e in self.0.lock().unwrap().iter_mut().filter(|e| e.write_id == write_id) {
                e.state = eengine::outbox::OutboxState::Acked;
                e.acked_at = Some(at.to_string());
            }
            Ok(())
        }
        fn remove(&self, write_ids: &[String]) -> Result<(), Failure> {
            self.0.lock().unwrap().retain(|e| !write_ids.contains(&e.write_id));
            Ok(())
        }
    }

    /// The server takes every write, answering the id it was given.
    struct Server(Mutex<Vec<(String, String, Value)>>);

    impl SyncTransport for Server {
        fn push(&self, app: &str, call: &str, input: &Value) -> OutboxResult {
            self.0.lock().unwrap().push((app.into(), call.into(), input.clone()));
            OutboxResult::Ok
        }
        fn send(&self, app: &str, call: &str, input: &Value) -> Pushed {
            let result = self.push(app, call, input);
            Pushed { answer: json!({"id": input.get("newId").cloned().unwrap_or(Value::Null)}), ..Pushed::of(result) }
        }
    }

    struct Member(&'static str);

    impl Identity for Member {
        fn identity(&self) -> EngineIdentity {
            EngineIdentity::Member { user_id: "u1".into(), role: Some(self.0.into()), access: None }
        }
    }

    fn connected(role: &'static str) -> (Runner, Arc<SyncedRecordStore>, Arc<Server>) {
        let server = Arc::new(Server(Mutex::default()));
        let store = Arc::new(SyncedRecordStore::new(Arc::new(MemoryLog::default()), server.clone(), Arc::new(SystemClock), None).unwrap());
        let synced = Synced { store: store.clone(), app: "app-uuid".into(), online: Arc::new(std::sync::atomic::AtomicBool::new(true)) };
        let providers = Providers { identity: Some(Arc::new(Member(role))), synced: Some(synced), ..providers() };
        let mut r = Runner::boot(config(), &package(), providers, FormFactor::Wide).expect("boots");
        let _ = r.launch(Vec::new());
        (r, store, server)
    }

    #[test]
    fn a_write_goes_through_the_synced_outbox_under_the_servers_app_id() {
        let (mut r, store, server) = connected("member");
        let Dispatch::Synced { synced, entry } = r.dispatch("records_upsert", &json!({"kind": "note", "data": {"text": "hi"}})) else { panic!("a synced write") };
        assert_eq!(synced.app, "app-uuid");
        let result = submitted(&synced, entry);
        let CallResult::Value(value) = result else { panic!("{result:?}") };
        let id = value["id"].as_str().unwrap().to_string();
        let sent = server.0.lock().unwrap();
        assert_eq!((sent[0].0.as_str(), sent[0].1.as_str()), ("app-uuid", "upsert"));
        assert_eq!(sent[0].2["newId"], json!(id));
        drop(sent);
        // The queued row shows through the instance's reads until the shape reflects it.
        let notes = r.records("records:note").unwrap();
        assert_eq!(notes["count"], json!(1));
        assert_eq!(notes["rows"][0]["text"], json!("hi"));
        store.set_rows("app-uuid", vec![RecordRow { id: id.clone(), kind: "note".into(), data: json!({"text": "hi"}), created_by: Some("u1".into()), author_id: Some("u1".into()), created_at: "2026-11-07T10:00:00.000Z".into(), updated_at: "2026-11-07T10:00:00.000Z".into() }]);
        store.prune();
        assert_eq!(r.records("records:note").unwrap()["count"], json!(1));
    }

    #[test]
    fn the_kind_policy_is_judged_as_the_member_before_the_queue() {
        let (mut r, _, server) = connected("member");
        // The sample's `note` kind lets members create; a delete of someone else's row is the author's alone.
        let Dispatch::Done(CallResult::Refused(f)) = r.dispatch("records_delete", &json!({"id": "not-mine"})) else { panic!("refused") };
        assert_eq!(f.code, "FORBIDDEN");
        assert!(server.0.lock().unwrap().is_empty());
    }
}
