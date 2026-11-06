//! The pure parts with fakes behind every seam; every answer's
//! `structuredContent` is checked against capabilities.json's
//! `nativeOutputSchema` (read from the repo's fixture, compiled into eengine).

use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use base64::Engine as _;
use chrono::{TimeZone, Utc};
use eengine::contract::CAPABILITIES;
use eengine::Capability;
use serde_json::{json, Value};

use crate::notifications::NotificationsCapability;
use crate::photos::{decode_cursor, encode_cursor, safe_relative};
use crate::*;

// ---- a minimal checker for the schema keywords nativeOutputSchema uses ----

fn type_ok(value: &Value, name: &str) -> bool {
    match name {
        "object" => value.is_object(),
        "array" => value.is_array(),
        "string" => value.is_string(),
        "boolean" => value.is_boolean(),
        "null" => value.is_null(),
        "integer" => value.as_f64().is_some_and(|n| n.fract() == 0.0),
        "number" => value.is_number(),
        other => panic!("schema type {other} unsupported by the test checker"),
    }
}

fn format_ok(text: &str, format: &str) -> bool {
    match format {
        "base64" => base64::engine::general_purpose::STANDARD.decode(text).is_ok() && text.len() % 4 == 0,
        "date-time" => chrono::DateTime::parse_from_rfc3339(text).is_ok(),
        "token" => (1..=64).contains(&text.len()) && text.chars().all(|c| c.is_ascii_alphanumeric() || "._-".contains(c)),
        "uri" => text.starts_with("https://") || text.starts_with("http://"),
        other => panic!("format {other} unsupported by the test checker"),
    }
}

fn check(value: &Value, schema: &Value, path: &str) -> Result<(), String> {
    let fail = |keyword: &str| Err(format!("{} {keyword}", if path.is_empty() { "/" } else { path }));
    if let Some(kind) = schema.get("type") {
        let ok = match kind {
            Value::String(name) => type_ok(value, name),
            Value::Array(names) => names.iter().any(|n| type_ok(value, n.as_str().unwrap())),
            _ => true,
        };
        if !ok {
            return fail("type");
        }
    }
    if let Some(options) = schema.get("enum").and_then(Value::as_array) {
        if !options.contains(value) {
            return fail("enum");
        }
    }
    if let Some(text) = value.as_str() {
        let length = text.chars().count() as u64;
        if schema.get("minLength").and_then(Value::as_u64).is_some_and(|min| length < min) {
            return fail("minLength");
        }
        if schema.get("maxLength").and_then(Value::as_u64).is_some_and(|max| length > max) {
            return fail("maxLength");
        }
        if let Some(format) = schema.get("format").and_then(Value::as_str) {
            if !format_ok(text, format) {
                return fail("format");
            }
        }
    }
    if let Some(number) = value.as_f64() {
        if schema.get("minimum").and_then(Value::as_f64).is_some_and(|min| number < min) {
            return fail("minimum");
        }
        if schema.get("maximum").and_then(Value::as_f64).is_some_and(|max| number > max) {
            return fail("maximum");
        }
    }
    if let Some(object) = value.as_object() {
        for key in schema.get("required").and_then(Value::as_array).into_iter().flatten() {
            if !object.contains_key(key.as_str().unwrap()) {
                return Err(format!("{path}/{} required", key.as_str().unwrap()));
            }
        }
        let properties = schema.get("properties").and_then(Value::as_object);
        if schema.get("additionalProperties") == Some(&Value::Bool(false)) {
            if let Some(extra) = object.keys().find(|k| !properties.is_some_and(|p| p.contains_key(*k))) {
                return Err(format!("{path}/{extra} additionalProperties"));
            }
        }
        for (key, sub) in properties.into_iter().flatten() {
            if let Some(child) = object.get(key) {
                check(child, sub, &format!("{path}/{key}"))?;
            }
        }
    }
    if let Some(items) = value.as_array() {
        if schema.get("minItems").and_then(Value::as_u64).is_some_and(|min| (items.len() as u64) < min) {
            return fail("minItems");
        }
        if schema.get("maxItems").and_then(Value::as_u64).is_some_and(|max| (items.len() as u64) > max) {
            return fail("maxItems");
        }
        if let Some(item_schema) = schema.get("items") {
            for (index, item) in items.iter().enumerate() {
                check(item, item_schema, &format!("{path}/{index}"))?;
            }
        }
    }
    Ok(())
}

fn tool(name: &str) -> &'static Value {
    CAPABILITIES["tools"].as_array().unwrap().iter().find(|t| t["name"] == name).unwrap_or_else(|| panic!("no tool {name}"))
}

/// A successful answer: matches the native schema, text = the same JSON.
fn structured(name: &str, result: &Value) -> Value {
    assert!(result.get("isError").is_none(), "{name} answered an error: {result}");
    let content = &result["structuredContent"];
    check(content, &tool(name)["nativeOutputSchema"], "").unwrap_or_else(|e| panic!("{name}: {e} in {content}"));
    let text: Value = serde_json::from_str(result["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(&text, content);
    assert_eq!(result["content"][0]["type"], "text");
    content.clone()
}

fn is_error(result: &Value) -> String {
    assert_eq!(result["isError"], true, "expected an error: {result}");
    assert!(result.get("structuredContent").is_none());
    result["content"][0]["text"].as_str().unwrap().to_string()
}

fn b64(bytes: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

// ---- fakes ----

#[derive(Default)]
struct FakeDialogs {
    open_answer: Mutex<Option<NativeResult<Option<Vec<PathBuf>>>>>,
    save_answer: Mutex<Option<Option<PathBuf>>>,
    opened: Mutex<Vec<OpenRequest>>,
    saved: Mutex<Vec<SaveRequest>>,
}

impl FakeDialogs {
    fn opening(paths: Option<Vec<PathBuf>>) -> Arc<Self> {
        let fake = FakeDialogs::default();
        *fake.open_answer.lock().unwrap() = Some(Ok(paths));
        Arc::new(fake)
    }
}

impl Dialogs for FakeDialogs {
    fn open(&self, request: &OpenRequest) -> NativeResult<Option<Vec<PathBuf>>> {
        self.opened.lock().unwrap().push(request.clone());
        self.open_answer.lock().unwrap().clone().unwrap_or(Ok(None))
    }

    fn save(&self, request: &SaveRequest) -> NativeResult<Option<PathBuf>> {
        self.saved.lock().unwrap().push(request.clone());
        Ok(self.save_answer.lock().unwrap().clone().flatten())
    }
}

struct FakeShare {
    answer: bool,
    items: Mutex<Vec<ShareItem>>,
}

impl ShareSheet for FakeShare {
    fn share(&self, item: &ShareItem) -> NativeResult<bool> {
        self.items.lock().unwrap().push(item.clone());
        Ok(self.answer)
    }
}

struct FakePrinter {
    answer: NativeResult<bool>,
    jobs: Mutex<Vec<NamedBytes>>,
}

impl Printer for FakePrinter {
    fn print(&self, job: &NamedBytes) -> NativeResult<bool> {
        self.jobs.lock().unwrap().push(job.clone());
        self.answer.clone()
    }
}

type OnEvent = Box<dyn FnOnce(bool) + Send>;

#[derive(Default)]
struct FakeNotifier {
    shown: Mutex<Vec<(Shown, ShownHandle)>>,
    events: Mutex<Vec<(ShownHandle, Option<OnEvent>)>>,
    closed: Mutex<Vec<ShownHandle>>,
    fail: Mutex<bool>,
}

impl FakeNotifier {
    /// The platform reports a click (`true`) or dismissal of the `n`th shown one.
    fn event(&self, n: usize, clicked: bool) {
        let callback = self.events.lock().unwrap()[n].1.take().expect("one event per notification");
        callback(clicked);
    }

    fn shown_titles(&self) -> Vec<String> {
        self.shown.lock().unwrap().iter().map(|(s, _)| s.title.clone()).collect()
    }
}

impl Notifier for FakeNotifier {
    fn show(&self, notification: &Shown, on_event: OnEvent) -> NativeResult<ShownHandle> {
        if *self.fail.lock().unwrap() {
            return Err(NativeError::Failed("no notification server".into()));
        }
        let mut shown = self.shown.lock().unwrap();
        let handle = ShownHandle(shown.len() as u64 + 100);
        shown.push((notification.clone(), handle));
        self.events.lock().unwrap().push((handle, Some(on_event)));
        Ok(handle)
    }

    fn close(&self, handle: ShownHandle) {
        self.closed.lock().unwrap().push(handle);
    }
}

fn touch(path: &Path, bytes: &[u8], age_secs: u64) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, bytes).unwrap();
    let file = File::options().write(true).open(path).unwrap();
    file.set_modified(SystemTime::UNIX_EPOCH + Duration::from_secs(1_800_000_000 - age_secs)).unwrap();
}

/// A Pictures folder: four library photos plus what the scan must skip.
fn library() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    touch(&root.join("old.jpg"), b"\xFF\xD8\xFFold", 400);
    touch(&root.join("Trips/beach.PNG"), b"\x89PNG\r\n\x1a\nbeach", 100);
    touch(&root.join("Trips/2026/sunset.heic"), b"heic", 200);
    touch(&root.join("cat.webp"), b"webp", 300);
    touch(&root.join(".hidden.jpg"), b"x", 1);
    touch(&root.join(".thumbs/t.jpg"), b"x", 1);
    touch(&root.join("Photos Library.photoslibrary/originals/a.jpg"), b"x", 1);
    touch(&root.join("notes.txt"), b"x", 1);
    touch(&root.join("raw.cr2"), b"x", 1);
    dir
}

// ---- tools ----

#[test]
fn every_capability_lists_exactly_its_native_tools() {
    let names = |cap: &dyn Capability| cap.tools().iter().map(|t| t["name"].as_str().unwrap().to_string()).collect::<Vec<_>>();
    let dialogs = FakeDialogs::opening(None);
    assert_eq!(names(&PhotosCapability::new(None, None)), ["get", "list", "pick"]);
    assert_eq!(names(&FilesCapability::new(dialogs)), ["open", "save"]);
    assert_eq!(names(&ShareCapability::new(Arc::new(FakeShare { answer: true, items: Mutex::default() }))), ["send"]);
    // print.pdf is the Engine's: never a native tool.
    assert_eq!(names(&PrintCapability::new(Arc::new(FakePrinter { answer: Ok(true), jobs: Mutex::default() }))), ["print"]);
    let notifications = NotificationsCapability::build(Arc::new(FakeNotifier::default()), Arc::new(|_| {}), false);
    assert_eq!(names(&notifications), ["cancel", "post"]);
    for t in PhotosCapability::new(None, None).tools() {
        let catalog = tool(&format!("photos.{}", t["name"].as_str().unwrap()));
        assert_eq!(t["inputSchema"], catalog["inputSchema"]);
        assert_eq!(t["description"], catalog["description"]);
    }
}

#[test]
fn an_unknown_tool_is_an_rpc_error() {
    let failure = PhotosCapability::new(None, None).call("delete", &json!({})).unwrap_err();
    assert_eq!(failure.code, "UNKNOWN_TOOL");
}

#[test]
fn the_checker_accepts_the_fixtures_native_results_and_refuses_bad_ones() {
    for case in CAPABILITIES["bridgeCases"].as_array().unwrap() {
        if let Some(content) = case["nativeResult"].get("structuredContent") {
            let name = case["tool"].as_str().unwrap();
            let schema = &tool(name)["nativeOutputSchema"];
            // A case may hand a bad result on purpose (the bridge's badResult).
            let expected_bad = case["expected"]["isError"].as_str().is_some_and(|e| e.starts_with("badResult"));
            assert_eq!(check(content, schema, "").is_err(), expected_bad, "{}", case["name"]);
        }
    }
    let schema = &tool("photos.list")["nativeOutputSchema"];
    assert!(check(&json!({"photos": [], "cursor": null, "extra": 1}), schema, "").is_err());
    assert!(check(&json!({"photos": [{"id": "a", "name": "a", "mimeType": "image/png", "takenAt": "yesterday"}], "cursor": null}), schema, "").is_err());
}

// ---- photos ----

#[test]
fn list_pages_the_folder_newest_first_without_bytes() {
    let dir = library();
    let photos = PhotosCapability::new(Some(dir.path().to_path_buf()), None);
    let first = structured("photos.list", &photos.call("list", &json!({"limit": 3})).unwrap());
    let ids: Vec<&str> = first["photos"].as_array().unwrap().iter().map(|p| p["id"].as_str().unwrap()).collect();
    assert_eq!(ids, ["Trips/beach.PNG", "Trips/2026/sunset.heic", "cat.webp"]);
    assert_eq!(first["photos"][0]["mimeType"], "image/png");
    assert_eq!(first["photos"][1]["mimeType"], "image/heic");
    assert_eq!(first["photos"][0]["name"], "beach.PNG");
    assert_eq!(first["photos"][0]["takenAt"], "2027-01-15T07:58:20Z");
    assert!(first["photos"][0].get("data").is_none());
    let cursor = first["cursor"].as_str().unwrap();
    let second = structured("photos.list", &photos.call("list", &json!({"limit": 3, "cursor": cursor})).unwrap());
    assert_eq!(second["photos"].as_array().unwrap().len(), 1);
    assert_eq!(second["photos"][0]["id"], "old.jpg");
    assert_eq!(second["cursor"], Value::Null);
}

#[test]
fn list_defaults_to_fifty_and_an_absent_folder_is_empty() {
    let dir = library();
    let photos = PhotosCapability::new(Some(dir.path().to_path_buf()), None);
    let all = structured("photos.list", &photos.call("list", &json!({})).unwrap());
    assert_eq!(all["photos"].as_array().unwrap().len(), 4);
    assert_eq!(all["cursor"], Value::Null);
    let none = PhotosCapability::new(Some(dir.path().join("missing")), None);
    assert_eq!(structured("photos.list", &none.call("list", &json!({})).unwrap()), json!({"photos": [], "cursor": null}));
}

#[test]
fn a_bad_cursor_fails() {
    let photos = PhotosCapability::new(None, None);
    assert_eq!(is_error(&photos.call("list", &json!({"cursor": "nope"})).unwrap()), "failed: invalid cursor");
    assert_eq!(decode_cursor(&encode_cursor(12345)), Some(12345));
    assert_eq!(decode_cursor("o"), None);
}

#[test]
fn get_answers_the_bytes_and_refuses_anything_outside_the_folder() {
    let dir = library();
    let photos = PhotosCapability::new(Some(dir.path().to_path_buf()), None);
    let photo = structured("photos.get", &photos.call("get", &json!({"id": "Trips/beach.PNG"})).unwrap());
    assert_eq!(photo["photo"]["data"], b64(b"\x89PNG\r\n\x1a\nbeach"));
    assert_eq!(photo["photo"]["id"], "Trips/beach.PNG");
    std::fs::write(dir.path().parent().unwrap().join("outside.jpg"), b"x").ok();
    for id in ["gone.jpg", "../outside.jpg", "/etc/passwd", "Trips/../old.jpg", "./old.jpg", "notes.txt", "Trips", "C:\\x.jpg", "Trips//beach.PNG"] {
        assert_eq!(is_error(&photos.call("get", &json!({"id": id})).unwrap()), "notFound", "{id}");
    }
}

#[cfg(unix)]
#[test]
fn a_symlink_out_of_the_folder_is_not_found() {
    let dir = library();
    let outside = tempfile::tempdir().unwrap();
    std::fs::write(outside.path().join("secret.jpg"), b"x").unwrap();
    std::os::unix::fs::symlink(outside.path().join("secret.jpg"), dir.path().join("link.jpg")).unwrap();
    let photos = PhotosCapability::new(Some(dir.path().to_path_buf()), None);
    assert_eq!(is_error(&photos.call("get", &json!({"id": "link.jpg"})).unwrap()), "notFound");
    let listed = structured("photos.list", &photos.call("list", &json!({})).unwrap());
    assert!(!listed["photos"].as_array().unwrap().iter().any(|p| p["id"] == "link.jpg"));
}

#[test]
fn ids_are_relative_paths_only() {
    assert_eq!(safe_relative("a/b.jpg"), Some(PathBuf::from("a").join("b.jpg")));
    for bad in ["", "/a.jpg", "a/../b.jpg", "..", "a//b", "a\\b", "C:x"] {
        assert_eq!(safe_relative(bad), None, "{bad}");
    }
}

#[test]
fn pick_filters_to_images_caps_at_limit_and_ids_resolve() {
    let dir = library();
    let outside = tempfile::tempdir().unwrap();
    let elsewhere = outside.path().join("IMG_0001.jpg");
    touch(&elsewhere, b"\xFF\xD8\xFFhi", 10);
    let dialogs = FakeDialogs::opening(Some(vec![dir.path().join("cat.webp"), elsewhere.clone(), dir.path().join("old.jpg")]));
    let photos = PhotosCapability::new(Some(dir.path().to_path_buf()), Some(dialogs.clone()));
    let picked = structured("photos.pick", &photos.call("pick", &json!({"limit": 2})).unwrap());
    let list = picked["photos"].as_array().unwrap();
    assert_eq!(list.len(), 2);
    assert_eq!(list[0]["id"], "cat.webp");
    assert_eq!(list[0]["mimeType"], "image/webp");
    assert_eq!(list[1]["id"], "picked:1");
    assert_eq!(list[1]["data"], b64(b"\xFF\xD8\xFFhi"));
    let request = dialogs.opened.lock().unwrap()[0].clone();
    assert!(request.multiple);
    assert_eq!(request.filter.extensions, ["gif", "heic", "jpeg", "jpg", "png", "webp"]);
    assert!(request.filter.mime_types.contains(&"image/heic".to_string()));
    // The picked id reads back through get (this process).
    let again = structured("photos.get", &photos.call("get", &json!({"id": "picked:1"})).unwrap());
    assert_eq!(again["photo"]["name"], "IMG_0001.jpg");
    // limit 1 (the default) = a single-selection picker.
    structured("photos.pick", &photos.call("pick", &json!({})).unwrap());
    assert!(!dialogs.opened.lock().unwrap()[1].multiple);
}

#[test]
fn a_cancelled_or_missing_picker() {
    let photos = PhotosCapability::new(None, Some(FakeDialogs::opening(None)));
    assert_eq!(structured("photos.pick", &photos.call("pick", &json!({})).unwrap()), json!({"photos": []}));
    let none = PhotosCapability::new(None, None);
    assert_eq!(is_error(&none.call("pick", &json!({})).unwrap()), "unavailable");
}

// ---- files ----

#[test]
fn open_reads_the_picked_files_with_their_types() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("a.csv"), b"a,b").unwrap();
    std::fs::write(dir.path().join("scan"), b"%PDF-1.4\n").unwrap();
    std::fs::write(dir.path().join("blob"), b"\x00\x01").unwrap();
    let paths = vec![dir.path().join("a.csv"), dir.path().join("scan"), dir.path().join("blob")];
    let dialogs = FakeDialogs::opening(Some(paths));
    let files = FilesCapability::new(dialogs.clone());
    let opened = structured("files.open", &files.call("open", &json!({"accept": ["image/*", ".CSV"], "multiple": true})).unwrap());
    assert_eq!(
        opened,
        json!({"files": [
            {"name": "a.csv", "mimeType": "text/csv", "data": b64(b"a,b")},
            {"name": "scan", "mimeType": "application/pdf", "data": b64(b"%PDF-1.4\n")},
            {"name": "blob", "mimeType": "application/octet-stream", "data": b64(b"\x00\x01")},
        ]})
    );
    let filter = dialogs.opened.lock().unwrap()[0].filter.clone();
    assert_eq!(filter.mime_types, ["image/*"]);
    assert!(filter.extensions.contains(&"csv".to_string()) && filter.extensions.contains(&"png".to_string()) && filter.extensions.contains(&"svg".to_string()));
    // Not multiple: one file at most.
    let single = structured("files.open", &files.call("open", &json!({})).unwrap());
    assert_eq!(single["files"].as_array().unwrap().len(), 1);
    assert!(dialogs.opened.lock().unwrap()[1].filter.is_any());
}

#[test]
fn an_unmappable_type_offers_every_file_on_extension_platforms() {
    let filter = crate::mime::filter(&["application/x-thing".into(), ".csv".into()]);
    assert!(filter.extensions.is_empty());
    assert_eq!(filter.mime_types, ["application/x-thing"]);
    assert!(crate::mime::filter(&["*/*".into()]).is_any());
    assert_eq!(crate::mime::filter(&["text/csv".into()]).extensions, ["csv"]);
}

#[test]
fn a_cancelled_open_is_an_empty_list() {
    let files = FilesCapability::new(FakeDialogs::opening(None));
    assert_eq!(structured("files.open", &files.call("open", &json!({"multiple": true})).unwrap()), json!({"files": []}));
}

#[test]
fn save_writes_where_the_person_picked() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("notes.txt");
    let dialogs = Arc::new(FakeDialogs::default());
    *dialogs.save_answer.lock().unwrap() = Some(Some(target.clone()));
    let files = FilesCapability::new(dialogs.clone());
    let saved = structured("files.save", &files.call("save", &json!({"name": "notes.txt", "mimeType": "text/plain", "data": "aGVsbG8="})).unwrap());
    assert_eq!(saved, json!({"saved": true}));
    assert_eq!(std::fs::read(&target).unwrap(), b"hello");
    assert_eq!(dialogs.saved.lock().unwrap()[0], SaveRequest { name: "notes.txt".into(), mime_type: "text/plain".into() });
    *dialogs.save_answer.lock().unwrap() = Some(None);
    let cancelled = structured("files.save", &files.call("save", &json!({"name": "x", "mimeType": "text/plain", "data": ""})).unwrap());
    assert_eq!(cancelled, json!({"saved": false}));
    assert_eq!(is_error(&files.call("save", &json!({"name": "x", "mimeType": "text/plain", "data": "!!"})).unwrap()), "failed: bad base64 data");
}

// ---- share + print ----

#[test]
fn share_hands_the_sheet_the_decoded_file() {
    let sheet = Arc::new(FakeShare { answer: false, items: Mutex::default() });
    let share = ShareCapability::new(sheet.clone());
    let args = json!({"title": "Album", "text": "Look", "url": "https://exponential.at", "file": {"name": "a.pdf", "mimeType": "application/pdf", "data": "JVBERi0xLjQK"}});
    assert_eq!(structured("share.send", &share.call("send", &args).unwrap()), json!({"shared": false}));
    let item = sheet.items.lock().unwrap()[0].clone();
    assert_eq!(item.title.as_deref(), Some("Album"));
    assert_eq!(item.url.as_deref(), Some("https://exponential.at"));
    assert_eq!(item.file.unwrap(), NamedBytes { name: "a.pdf".into(), mime_type: "application/pdf".into(), bytes: b"%PDF-1.4\n".to_vec() });
    assert_eq!(is_error(&share.call("send", &json!({"title": "only"})).unwrap()), "failed: nothing to share");
}

#[test]
fn print_takes_pdf_png_jpeg_and_answers_the_dialog() {
    let printer = Arc::new(FakePrinter { answer: Ok(false), jobs: Mutex::default() });
    let print = PrintCapability::new(printer.clone());
    let pdf = json!({"name": "document.pdf", "mimeType": "application/pdf", "data": "JVBERi0xLjQK"});
    assert_eq!(structured("print.print", &print.call("print", &pdf).unwrap()), json!({"printed": false}));
    assert_eq!(printer.jobs.lock().unwrap()[0].mime_type, "application/pdf");
    let text = json!({"name": "a.txt", "mimeType": "text/plain", "data": "aGk="});
    assert_eq!(is_error(&print.call("print", &text).unwrap()), "failed: not a PDF, PNG or JPEG");
    let failing = PrintCapability::new(Arc::new(FakePrinter { answer: Err(NativeError::Unavailable), jobs: Mutex::default() }));
    assert_eq!(is_error(&failing.call("print", &pdf).unwrap()), "unavailable");
}

// ---- notifications ----

fn center() -> (NotificationsCapability, Arc<FakeNotifier>, Arc<Mutex<Vec<Value>>>) {
    let notifier = Arc::new(FakeNotifier::default());
    let taps = Arc::new(Mutex::new(Vec::new()));
    let sink = taps.clone();
    let capability = NotificationsCapability::build(notifier.clone(), Arc::new(move |tap| sink.lock().unwrap().push(tap)), false);
    (capability, notifier, taps)
}

fn tap(screen: &str) -> Value {
    json!({"type": "notificationTap", "app": "acme.album", "screen": screen, "params": {"id": 7}})
}

fn post(id: &str, title: &str, at: Option<&str>, tap: Option<Value>) -> Value {
    json!({"id": id, "title": title, "body": null, "at": at, "tap": tap})
}

#[test]
fn a_tap_hands_the_stored_tap_back_verbatim() {
    let (capability, notifier, taps) = center();
    let answer = capability.call("post", &post("acme.album/n-1", "Album ready", None, Some(tap("page")))).unwrap();
    assert_eq!(structured("notifications.post", &answer), json!({}));
    assert_eq!(notifier.shown.lock().unwrap()[0].0, Shown { id: "acme.album/n-1".into(), title: "Album ready".into(), body: None });
    notifier.event(0, true);
    assert_eq!(*taps.lock().unwrap(), [tap("page")]);
    // Tapped = gone: nothing left to cancel.
    let cancelled = structured("notifications.cancel", &capability.call("cancel", &json!({"id": "acme.album/n-1"})).unwrap());
    assert_eq!(cancelled, json!({"cancelled": false}));
}

#[test]
fn a_dismissal_or_a_tapless_notification_never_calls_the_host() {
    let (capability, notifier, taps) = center();
    capability.call("post", &post("a/1", "One", None, Some(tap("page")))).unwrap();
    capability.call("post", &post("a/2", "Two", None, None)).unwrap();
    notifier.event(0, false);
    notifier.event(1, true);
    assert!(taps.lock().unwrap().is_empty());
}

#[test]
fn posting_an_id_again_replaces_it() {
    let (capability, notifier, taps) = center();
    capability.call("post", &post("a/1", "First", None, Some(tap("albums")))).unwrap();
    capability.call("post", &post("a/1", "Second", None, Some(tap("page")))).unwrap();
    assert_eq!(notifier.shown_titles(), ["First", "Second"]);
    assert_eq!(*notifier.closed.lock().unwrap(), [ShownHandle(100)]);
    // The replaced one's late click is dropped; the new one's lands.
    notifier.event(0, true);
    assert!(taps.lock().unwrap().is_empty());
    notifier.event(1, true);
    assert_eq!(*taps.lock().unwrap(), [tap("page")]);
}

#[test]
fn scheduled_ones_fire_at_their_time_and_past_times_post_now() {
    let (capability, notifier, _) = center();
    let now = Utc.with_ymd_and_hms(2026, 11, 6, 12, 0, 0).unwrap();
    capability.post_at(&post("a/later", "Later", Some("2026-11-06T14:00:00+01:00"), None), now).unwrap();
    capability.post_at(&post("a/past", "Past", Some("2026-11-06T11:00:00Z"), None), now).unwrap();
    capability.post_at(&post("a/far", "Far", Some("2026-11-07T00:00:00Z"), None), now).unwrap();
    assert_eq!(notifier.shown_titles(), ["Past"]);
    assert_eq!(capability.pending_count(), 2);
    capability.tick(now + chrono::Duration::minutes(59));
    assert_eq!(notifier.shown_titles(), ["Past"]);
    capability.tick(now + chrono::Duration::hours(1));
    assert_eq!(notifier.shown_titles(), ["Past", "Later"]);
    assert_eq!(capability.pending_count(), 1);
}

#[test]
fn cancel_withdraws_pending_and_shown_ones() {
    let (capability, notifier, _) = center();
    let now = Utc.with_ymd_and_hms(2026, 11, 6, 12, 0, 0).unwrap();
    capability.post_at(&post("a/soon", "Soon", Some("2026-11-06T12:05:00Z"), None), now).unwrap();
    capability.post_at(&post("a/now", "Now", None, None), now).unwrap();
    let cancel = |id: &str| structured("notifications.cancel", &capability.call("cancel", &json!({"id": id})).unwrap())["cancelled"].clone();
    assert_eq!(cancel("a/soon"), true);
    assert_eq!(cancel("a/now"), true);
    assert_eq!(*notifier.closed.lock().unwrap(), [ShownHandle(100)]);
    assert_eq!(cancel("a/now"), false);
    assert_eq!(cancel("a/never"), false);
    capability.tick(now + chrono::Duration::hours(1));
    assert_eq!(notifier.shown_titles(), ["Now"]);
}

#[test]
fn a_platform_failure_is_failed_and_leaves_nothing_behind() {
    let (capability, notifier, _) = center();
    *notifier.fail.lock().unwrap() = true;
    assert_eq!(is_error(&capability.call("post", &post("a/1", "x", None, None)).unwrap()), "failed: no notification server");
    let cancelled = structured("notifications.cancel", &capability.call("cancel", &json!({"id": "a/1"})).unwrap());
    assert_eq!(cancelled, json!({"cancelled": false}));
    assert_eq!(is_error(&capability.call("post", &post("a/2", "x", Some("tomorrow"), None)).unwrap()), "failed: bad at");
}

#[test]
fn the_scheduler_thread_fires_a_due_notification() {
    let notifier = Arc::new(FakeNotifier::default());
    let capability = NotificationsCapability::new(notifier.clone(), Arc::new(|_| {}));
    let at = (Utc::now() + chrono::Duration::milliseconds(300)).to_rfc3339();
    capability.call("post", &post("a/1", "Soon", Some(&at), None)).unwrap();
    assert!(notifier.shown_titles().is_empty());
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while notifier.shown_titles().is_empty() && std::time::Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(notifier.shown_titles(), ["Soon"]);
}

// ---- the OS set ----

#[test]
fn desktop_capabilities_names_what_this_os_provides() {
    let mut config = DesktopConfig::new("Exponential", Arc::new(|_| {}));
    config.pictures_dir = Some(std::env::temp_dir());
    let names: Vec<String> = desktop_capabilities(config).iter().map(|c| c.name().to_string()).collect();
    let expected: &[&str] = if cfg!(target_os = "macos") {
        &["photos", "files", "share", "print", "notifications"]
    } else if cfg!(target_os = "linux") {
        &["photos", "files", "print", "notifications"]
    } else if cfg!(target_os = "windows") {
        &["photos", "files", "notifications"]
    } else {
        &["photos"]
    };
    assert_eq!(names, expected);
}
