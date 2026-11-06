//! VAPP-271: the Photobook (samples/apps/photobook.app.json) end to end on
//! the Rust Engine, the twin of packages/eengine `photobook.test.ts`. A host
//! runs a package action the way `ExponentialHost` does (eui-core's
//! `run_action` until the plan is whole, each pending `result` call through
//! the instance first, then the writes as one `records.batch`, the sets, the
//! calls); the natives are fakes and nothing reaches the network.

use std::sync::{Arc, Mutex};

use eengine::capability::Capability;
use eengine::consent::ConsentAnswer;
use eengine::instance::{AppInstance, CallResult};
use eengine::lifecycle::LifecycleEvent;
use eengine::providers::{BlobStore, Clock, Failure, HostConfig, HostInfo, NetworkPolicy, Presentation, WindowHost};
use eengine::stores::{FsBlobStore, LocalIdentity, MemoryRecordStore, MemoryVault};
use serde_json::{json, Map, Value};

const FILE: &str = include_str!("../../../../../samples/apps/photobook.app.json");
const CAPABILITIES: &str = include_str!("../../../../../packages/eengine/fixtures/capabilities.json");

type Calls = Arc<Mutex<Vec<(String, Value)>>>;

/// A fake native: records every call, answers per tool.
struct Fake {
    name: &'static str,
    calls: Calls,
    answer: Box<dyn Fn(&str, &Value) -> Value + Send + Sync>,
}

impl Capability for Fake {
    fn name(&self) -> &str {
        self.name
    }
    fn tools(&self) -> Vec<Value> {
        vec![]
    }
    fn call(&self, tool: &str, arguments: &Value) -> Result<Value, Failure> {
        self.calls.lock().unwrap().push((tool.to_string(), arguments.clone()));
        Ok((self.answer)(tool, arguments))
    }
}

struct NoWindows;
impl WindowHost for NoWindows {
    fn open(&self, _app: &str, _screen: &str, _params: &Map<String, Value>, _how: Presentation) {}
}

struct FixedClock;
impl Clock for FixedClock {
    fn now(&self) -> String {
        "2026-11-06T10:00:00.000Z".into()
    }
}

struct Book {
    app: AppInstance,
    blobs: Arc<FsBlobStore>,
    photos: Calls,
    files: Calls,
    share: Calls,
    print: Calls,
    http: Calls,
    ids: usize,
}

fn fake(name: &'static str, answer: impl Fn(&str, &Value) -> Value + Send + Sync + 'static) -> (Arc<Fake>, Calls) {
    let calls: Calls = Arc::default();
    (Arc::new(Fake { name, calls: calls.clone(), answer: Box::new(answer) }), calls)
}

fn photobook() -> Book {
    let fixture: Value = serde_json::from_str(CAPABILITIES).unwrap();
    let blobs_case = fixture["pdfCases"].as_array().unwrap().iter().find(|c| c["blobs"]["jpg1"].is_string() && c["blobs"]["png1"].is_string()).unwrap();
    let (jpg, png) = (blobs_case["blobs"]["jpg1"].clone(), blobs_case["blobs"]["png1"].clone());
    let picks = Mutex::new(vec![
        json!([{"id": "lib-3", "name": "dock.png", "mimeType": "image/png", "takenAt": null, "data": png}]),
        json!([
            {"id": "lib-1", "name": "lake.jpg", "mimeType": "image/jpeg", "takenAt": null, "data": jpg},
            {"id": "lib-2", "name": "sky.png", "mimeType": "image/png", "takenAt": null, "data": png}
        ]),
    ]);
    let (photos, photo_calls) = fake("photos", move |_, _| json!({"structuredContent": {"photos": picks.lock().unwrap().pop().unwrap_or(json!([]))}}));
    let (files, file_calls) = fake("files", |_, _| json!({"structuredContent": {"saved": true}}));
    let (share, share_calls) = fake("share", |_, _| json!({"structuredContent": {"shared": true}}));
    let (print, print_calls) = fake("print", |_, _| json!({"structuredContent": {"printed": true}}));
    // Declared nowhere: a call would mean the package reached the network.
    let (http, http_calls) = fake("http", |_, _| json!({"content": [{"type": "text", "text": "unavailable"}], "isError": true}));
    let dir = std::env::temp_dir().join(format!("eengine-photobook-{}", std::process::id()));
    let blobs = Arc::new(FsBlobStore::new(dir));
    let config = HostConfig {
        host: HostInfo { kind: "exported".into(), platform: "linux".into(), form_factor: "desktop".into(), locale: "en".into() },
        storage: Arc::new(MemoryRecordStore::new()),
        blobs: blobs.clone(),
        identity: Arc::new(LocalIdentity { device_id: "d-1".into() }),
        vault: Arc::new(MemoryVault::default()),
        window: Arc::new(NoWindows),
        clock: Arc::new(FixedClock),
        zone: None,
        network: NetworkPolicy::Off,
        sync: None,
        capabilities: vec![photos, files, share, print, http],
        consent: None,
        audit: None,
    };
    let file: Value = serde_json::from_str(FILE).unwrap();
    let mut app = AppInstance::load(config, &file).expect("the Photobook loads");
    app.event(&LifecycleEvent::Launch);
    Book { app, blobs, photos: photo_calls, files: file_calls, share: share_calls, print: print_calls, http: http_calls, ids: 0 }
}

impl Book {
    /// One host function call from the UI; an ask is answered `allow`.
    fn call(&mut self, name: &str, args: &Value) -> Result<Value, String> {
        let creates = if name == "records.batch" { args["writes"].as_array().map_or(0, |w| w.iter().filter(|w| w["op"] != "delete" && !w["id"].is_string()).count()) } else { 0 };
        let ids: Vec<String> = (0..creates).map(|_| {
            self.ids += 1;
            format!("r{}", self.ids)
        }).collect();
        let mut result = self.app.call(name, args, &ids);
        loop {
            match result {
                CallResult::Value(v) => return Ok(v),
                CallResult::Refused(f) => return Err(f.message),
                CallResult::Ask(verdict, _) => result = self.app.answer_consent(name, args, &verdict, ConsentAnswer::Allow),
            }
        }
    }

    /// A press of `actions.<name>`: the host's loop (eui-gpui `run_package_action`).
    fn press(&mut self, action: &str, params: Value, data: &mut Value) -> Result<(), String> {
        let pkg = eengine::package::package_of(&serde_json::from_str::<Value>(FILE).unwrap()).clone();
        let snapshot = data.clone();
        let mut results = Map::new();
        let plan = loop {
            let ctx = eui_core::host::ActionContext { data: Some(&snapshot), me: Some("d-1"), now: None, offset: None, max_steps: None, results: Some(&results) };
            match eui_core::host::run_action(&pkg, action, &params, &ctx)? {
                eui_core::host::ActionRun::Plan(plan) => break plan,
                eui_core::host::ActionRun::Pending(p) => {
                    let answer = self.call(&p.name, &Value::Object(p.args.clone()))?;
                    results.insert(p.result.clone(), answer);
                }
            }
        };
        if !plan["writes"].as_array().unwrap().is_empty() {
            self.call("records.batch", &json!({"writes": plan["writes"]}))?;
        }
        for set in plan["sets"].as_array().unwrap() {
            *data.pointer_mut(set["path"].as_str().unwrap()).unwrap_or_else(|| panic!("{set}")) = set["value"].clone();
        }
        for c in plan["calls"].as_array().unwrap() {
            self.call(c["name"].as_str().unwrap(), &c["args"])?;
        }
        Ok(())
    }

    fn rows(&self, source: &str) -> Vec<Value> {
        self.app.records_value(source).unwrap()["rows"].as_array().unwrap().clone()
    }
}

#[test]
fn an_album_from_picked_photos_its_pdf_then_save_and_share() {
    let mut b = photobook();
    let pkg = eengine::package::package_of(&serde_json::from_str::<Value>(FILE).unwrap()).clone();
    assert_eq!(pkg["catalogId"], "https://ui.exponential.at/catalogs/core/v1");

    // Albums: New album.
    let mut albums = json!({"albums": {"rows": [], "count": 0}, "draft": "Summer 2026"});
    b.press("newAlbum", json!({"title": "Summer 2026"}), &mut albums).unwrap();
    assert_eq!(albums["draft"], "");
    let album = b.rows("records:album")[0].clone();
    let id = album["id"].as_str().unwrap().to_string();

    // The album: a page per pick, then photos onto it.
    let pages_source = format!("records:page?album={id}&sort=createdAt&order=asc&limit=200");
    let mut data = json!({"pages": {"rows": [], "count": 0}, "caption": "At the lake", "pdf": null});
    b.press("addPage", json!({"album": id, "caption": "At the lake", "cover": ""}), &mut data).unwrap();
    let page = b.rows(&pages_source)[0].clone();
    assert_eq!(page["photos"].as_array().unwrap().len(), 2);
    assert_eq!(page["caption"], "At the lake");
    let cover = page["photos"][0].as_str().unwrap().to_string();
    assert_eq!(b.rows("records:album")[0]["cover"], json!(cover), "the first photo is the cover");
    b.press("addPhotos", json!({"page": page["id"], "photos": page["photos"]}), &mut data).unwrap();
    let page = b.rows(&pages_source)[0].clone();
    assert_eq!(page["photos"].as_array().unwrap().len(), 3);
    // A cancelled pick adds nothing.
    assert_eq!(b.press("addPage", json!({"album": id, "cover": cover}), &mut data).err().as_deref(), Some("No photos picked"));
    assert_eq!(b.rows(&pages_source).len(), 1);

    // Export PDF: the package's action builds the blocks from the records.
    data["pages"] = b.app.records_value(&pages_source).unwrap();
    b.press("exportPdf", json!({"title": "Summer 2026", "cover": cover}), &mut data).unwrap();
    let pdf = data["pdf"].clone();
    assert_eq!((pdf["name"].as_str(), pdf["mimeType"].as_str()), (Some("Summer 2026.pdf"), Some("application/pdf")));
    let bytes = b.blobs.get("at.exponential.photobook", pdf["fileId"].as_str().unwrap()).unwrap().expect("the PDF is an app file");
    assert_eq!(bytes.len() as u64, pdf["size"].as_u64().unwrap());
    let body: String = bytes.iter().map(|&c| c as char).collect();
    assert!(body.starts_with("%PDF-1.4\n") && body.trim_end().ends_with("%%EOF"));
    assert!(body.contains("(Summer 2026) Tj") && body.contains("(At the lake) Tj"));
    assert_eq!(body.matches("/Subtype /Image").count(), 3, "three picked files");
    assert_eq!(body.matches(" Do Q").count(), 4, "drawn four times: the cover and three photos");
    assert!(body.contains("/DCTDecode") && body.contains("/FlateDecode"));

    // Save and Share the produced file.
    let data64 = eengine::tools::base64_encode(&bytes);
    assert_eq!(b.call("files.save", &json!({"fileId": pdf["fileId"], "name": pdf["name"]})).unwrap(), json!({"saved": true}));
    assert_eq!(b.files.lock().unwrap().clone(), vec![("save".to_string(), json!({"name": "Summer 2026.pdf", "mimeType": "application/pdf", "data": data64}))]);
    assert_eq!(b.call("share.send", &json!({"fileId": pdf["fileId"], "name": pdf["name"], "mimeType": "application/pdf"})).unwrap(), json!({"shared": true}));
    assert_eq!(b.share.lock().unwrap()[0].1["file"]["data"], json!(data64));

    // No network: no http call; print.pdf runs in the Engine.
    assert!(b.http.lock().unwrap().is_empty());
    assert!(b.print.lock().unwrap().is_empty());
    assert_eq!(b.photos.lock().unwrap().len(), 3);
}
