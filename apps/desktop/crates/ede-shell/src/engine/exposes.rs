//! The apps' `exposes` tools on the desktop agent's hub (#VAPP-273, spec §8
//! and §16 eDE item 3): every running app's exposed tools, listed as
//! `<app server>__<tool>` next to the noun subset and `shell_edit`, run
//! here as the web's apps broker runs them (apps/web/src/lib/emcp/
//! apps-broker.ts, the reference): a `records` run reads the app's rows
//! through the same records query (`list` with a named query and the run's
//! own keys, `get` by id) or writes through the Engine instance
//! (`upsert`: the kind policy, the store, the live sources); an `action`
//! run is one of Exponential's own tools, here the Local desktop's noun
//! subset, called "via" the app; `upstream` and `agent` runs have no seam on
//! a desktop and stay unlisted. The person's own agent on their own desktop
//! holds `write`, so every `read` and `write` tool shows; `admin` never does.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use eengine::providers::{Clock, RecordStore};
use eengine_runner::paint::Inbound;
use emcp::contract::access::{tool_visible, Level};
use emcp::contract::actions::parse_action_name;
use emcp::contract::exposes::{provenance_meta, qualified_tool_name, resolve_args, run_via, ToolAccess};
use emcp::contract::hub::server_name_valid;
use emcp::server::{Caller, ToolProvider};
use eui_core::host::{fill_query_templates, kind_computed, kind_refs, kind_transitions, parse_records_json, records_value_owned, RecordsContext};
use serde_json::{json, Map, Value};

/// Writes through the running instance (its kind policy, its store, its
/// live sources): the shell sends `records_upsert` into the app's main-thread
/// pump and waits; a test calls the runner directly.
pub trait Writes: Send + Sync {
    fn upsert(&self, args: &Value) -> Result<Value, String>;
}

/// The shell's writes: one `Inbound::Call` into the app's pump, answered
/// through the reply channel (the ticket space above the host's own).
pub struct PumpWrites(pub flume::Sender<Inbound>);

static TICKET: AtomicU64 = AtomicU64::new(1 << 40);

impl Writes for PumpWrites {
    fn upsert(&self, args: &Value) -> Result<Value, String> {
        let (reply, answer) = flume::bounded(1);
        let ticket = TICKET.fetch_add(1, Ordering::SeqCst);
        self.0.send(Inbound::Call { ticket, name: "records_upsert".into(), args: args.clone(), reply }).map_err(|_| "the app is gone".to_string())?;
        answer.recv_timeout(std::time::Duration::from_secs(30)).map_err(|_| "the app did not answer".to_string())?
    }
}

/// One app's seat on the hub.
pub struct AppTools {
    pub id: String,
    /// Its hub server name (`server_name`).
    pub server: String,
    pub package: Value,
    pub storage: Arc<dyn RecordStore>,
    /// The viewer (the device on a Local desktop).
    pub me: String,
    pub writes: Arc<dyn Writes>,
}

/// The hub server name of a package: its id with every other character a
/// dash (`at.exponential.notes` → `at-exponential-notes`), at most 32.
pub fn server_name(id: &str) -> String {
    let mut out = String::new();
    for c in id.chars().flat_map(char::to_lowercase) {
        let keep = c.is_ascii_lowercase() || c.is_ascii_digit();
        if keep {
            out.push(c);
        } else if !out.is_empty() && !out.ends_with('-') {
            out.push('-');
        }
    }
    let mut out: String = out.chars().take(32).collect();
    while out.ends_with('-') {
        out.pop();
    }
    if !server_name_valid(&out) {
        out = "app".into();
    }
    out
}

/// The exposes of a package this host lists: `records` and `action` runs,
/// `read` and `write` access (the person's own agent holds write).
fn listed(package: &Value) -> Vec<Value> {
    package
        .get("exposes")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|e| matches!(run_via(&e["run"]), "records" | "action"))
        .filter(|e| {
            let access = match e.get("access").and_then(Value::as_str) {
                Some("read") => ToolAccess::Read,
                Some("write") => ToolAccess::Write,
                _ => return false,
            };
            tool_visible(Level::Write, access)
        })
        .cloned()
        .collect()
}

fn answer(server: &str, expose: &Value, value: Value) -> Value {
    let structured = if value.is_object() { value } else { json!({"value": value}) };
    json!({"content": [{"type": "text", "text": structured.to_string()}], "structuredContent": structured, "_meta": provenance_meta(server, expose)})
}

fn error(server: &str, expose: &Value, code: &str, words: &str) -> Value {
    json!({"content": [{"type": "text", "text": format!("{code}: {words}")}], "isError": true, "_meta": provenance_meta(server, expose)})
}

/// The input's values as `${param}` texts: strings as they are, numbers and
/// booleans printed, the rest as JSON (the web's `templateParams`).
fn template_params(input: &Value) -> BTreeMap<String, String> {
    input
        .as_object()
        .into_iter()
        .flatten()
        .filter(|(_, v)| !v.is_null())
        .map(|(k, v)| (k.clone(), match v {
            Value::String(s) => s.clone(),
            Value::Number(_) | Value::Bool(_) => v.to_string(),
            other => other.to_string(),
        }))
        .collect()
}

const RUN_ONLY_KEYS: &[&str] = &["op", "kind", "id", "data", "query"];

fn is_uuid(s: &str) -> bool {
    uuid::Uuid::parse_str(s).is_ok()
}

/// Does `data` contain every `(key, value)` of `wanted` (the server's jsonb containment)?
fn contains(data: &Value, wanted: &Map<String, Value>) -> bool {
    wanted.iter().all(|(k, v)| data.get(k) == Some(v))
}

impl AppTools {
    pub fn tools_of(&self) -> Vec<Value> {
        listed(&self.package)
            .iter()
            .map(|e| {
                let name = e["name"].as_str().unwrap_or("");
                json!({
                    "name": qualified_tool_name(&self.server, name),
                    "title": e["title"],
                    "description": e["description"],
                    "inputSchema": e["input"],
                    "annotations": {"title": e["title"], "readOnlyHint": e["access"] == "read", "destructiveHint": false, "openWorldHint": false},
                    "_meta": provenance_meta(&self.server, e),
                })
            })
            .collect()
    }

    fn expose(&self, qualified: &str) -> Option<Value> {
        listed(&self.package).into_iter().find(|e| qualified_tool_name(&self.server, e["name"].as_str().unwrap_or("")) == qualified)
    }

    /// The app's rows as the viewer sees them.
    fn rows(&self) -> Result<Vec<eui_core::host::RecordRow>, String> {
        self.storage.rows(&self.id).map_err(|e| e.message)
    }

    fn records_context<'a>(&'a self, kind: &str, computed: &'a Option<BTreeMap<String, String>>, refs: &'a Option<Value>, transitions: &'a Option<eui_core::host::RecordTransitions>, now: &'a str, names: &'a std::collections::HashMap<String, String>) -> RecordsContext<'a> {
        let _ = kind;
        RecordsContext { me: Some(&self.me), now, today: None, names, computed: computed.as_ref(), refs: refs.as_ref(), issues: None, transitions: transitions.as_ref(), role: Some("owner") }
    }

    fn run_records(&self, expose: &Value, input: &Value, exponential: &Arc<dyn ToolProvider>, caller: &Caller) -> Value {
        let _ = (exponential, caller);
        let run = &expose["run"]["records"];
        let kind = run["kind"].as_str().unwrap_or("").to_string();
        let resolved = resolve_args(&json!({"id": run.get("id").cloned().unwrap_or(Value::Null), "data": run.get("data").cloned().unwrap_or(Value::Null)}), input).unwrap_or(Value::Null);
        match run["op"].as_str().unwrap_or("") {
            "get" => {
                let Some(id) = resolved.get("id").and_then(Value::as_str).filter(|s| is_uuid(s)) else {
                    return error(&self.server, expose, "invalidArguments", "a record id is a uuid");
                };
                let rows = match self.rows() {
                    Ok(r) => r,
                    Err(e) => return error(&self.server, expose, "toolError", &e),
                };
                match rows.into_iter().find(|r| r.id == id && r.kind == kind) {
                    Some(row) => answer(&self.server, expose, json!({"record": row_json(&row)})),
                    None => error(&self.server, expose, "toolError", "record not found"),
                }
            }
            "list" => {
                // The named query first, filled from the input; the run's own keys win; `where` as an object = containment.
                let mut query = Map::new();
                if let Some(name) = run.get("query") {
                    let named = name.as_str().and_then(|n| self.package.get("queries").and_then(|q| q.get(n))).filter(|q| q.is_object());
                    let Some(named) = named else {
                        return error(&self.server, expose, "toolError", &format!("the app has no query {name}"));
                    };
                    query = fill_query_templates(named, &template_params(input)).as_object().cloned().unwrap_or_default();
                }
                let own: Map<String, Value> = run.as_object().into_iter().flatten().filter(|(k, _)| !RUN_ONLY_KEYS.contains(&k.as_str())).map(|(k, v)| (k.clone(), v.clone())).collect();
                let mut resolved_own = resolve_args(&Value::Object(own), input).and_then(|v| v.as_object().cloned()).unwrap_or_default();
                let wanted = match resolved_own.remove("where") {
                    Some(Value::Object(w)) => w,
                    Some(other) => {
                        resolved_own.insert("where".into(), other);
                        Map::new()
                    }
                    None => Map::new(),
                };
                query.extend(resolved_own);
                if query.get("from").is_none_or(Value::is_null) {
                    query.insert("from".into(), json!(kind));
                }
                let (parsed, issues) = parse_records_json(&Value::Object(query), Some(&kind));
                if !issues.is_empty() {
                    let words: Vec<String> = issues.iter().map(|i| format!("{}: {}", i.path, i.message)).collect();
                    return error(&self.server, expose, "invalidArguments", &format!("the query is invalid: {}", words.join("; ")));
                }
                let rows = match self.rows() {
                    Ok(r) => r.into_iter().filter(|r| wanted.is_empty() || contains(&r.data, &wanted)).collect::<Vec<_>>(),
                    Err(e) => return error(&self.server, expose, "toolError", &e),
                };
                let records = self.package.get("records");
                let computed = kind_computed(records, Some(&kind));
                let refs = kind_refs(records, Some(&kind));
                let transitions = kind_transitions(records.and_then(|r| r.get(&kind)));
                let now = eengine::providers::SystemClock.now();
                let names = std::collections::HashMap::new();
                let ctx = self.records_context(&kind, &computed, &refs, &transitions, &now, &names);
                match records_value_owned(&rows, &parsed, ctx) {
                    Some(value) => answer(&self.server, expose, value),
                    None => error(&self.server, expose, "invalidArguments", "the query is invalid"),
                }
            }
            "upsert" => {
                let Some(data) = resolved.get("data").filter(|d| d.is_object()) else {
                    return error(&self.server, expose, "invalidArguments", "data is an object");
                };
                let mut args = json!({"kind": kind, "data": data});
                match resolved.get("id") {
                    None | Some(Value::Null) => {}
                    Some(Value::String(id)) if is_uuid(id) => args["id"] = json!(id),
                    Some(_) => return error(&self.server, expose, "invalidArguments", "a record id is a uuid"),
                }
                match self.writes.upsert(&args) {
                    Ok(value) => {
                        let id = value.get("id").cloned().unwrap_or(Value::Null);
                        answer(&self.server, expose, json!({"id": id, "created": resolved.get("id").is_none_or(Value::is_null)}))
                    }
                    Err(e) => error(&self.server, expose, "toolError", &e.chars().take(500).collect::<String>()),
                }
            }
            other => error(&self.server, expose, "toolError", &format!("unknown records op {other}")),
        }
    }

    /// An `action` run: one of Exponential's own tools, here the Local
    /// desktop's noun (`exponential_<noun>` with its `op`), called via the app.
    fn run_action(&self, expose: &Value, input: &Value, exponential: &Arc<dyn ToolProvider>, caller: &Caller) -> Value {
        let run = &expose["run"];
        let action = run["action"].as_str().unwrap_or("");
        let Some((tool, op)) = parse_action_name(action) else {
            return error(&self.server, expose, "notGranted", &format!("{action} is not an offered action"));
        };
        let mut args = resolve_args(run.get("args").unwrap_or(&json!({})), input).and_then(|v| v.as_object().cloned()).unwrap_or_default();
        if let Some(op) = op {
            args.insert("op".into(), json!(op));
        }
        let mut out = exponential.call(caller, &tool, Value::Object(args));
        if let Some(o) = out.as_object_mut() {
            o.insert("_meta".into(), Value::Object(provenance_meta(&self.server, expose)));
        }
        out
    }

    fn call_tool(&self, caller: &Caller, expose: &Value, input: &Value, exponential: &Arc<dyn ToolProvider>) -> Value {
        // The required keys of the tool's input, as the web's argumentsError.
        for key in expose["input"]["required"].as_array().into_iter().flatten().filter_map(Value::as_str) {
            if input.get(key).is_none_or(Value::is_null) {
                return error(&self.server, expose, "invalidArguments", &format!("{key} is required"));
            }
        }
        match run_via(&expose["run"]) {
            "records" => self.run_records(expose, input, exponential, caller),
            "action" => self.run_action(expose, input, exponential, caller),
            _ => error(&self.server, expose, "toolError", "unavailable on this desktop"),
        }
    }
}

fn row_json(row: &eui_core::host::RecordRow) -> Value {
    json!({"id": row.id, "kind": row.kind, "data": row.data, "createdBy": row.created_by, "authorId": row.author_id, "createdAt": row.created_at, "updatedAt": row.updated_at})
}

/// Every running app's exposed tools on one provider.
pub struct AppsProvider {
    pub apps: Vec<Arc<AppTools>>,
    /// Exponential's own tools on this desktop (the noun subset), for `action` runs.
    pub exponential: Arc<dyn ToolProvider>,
}

impl ToolProvider for AppsProvider {
    fn tools(&self) -> Result<Vec<Value>, String> {
        let mut out = Vec::new();
        let mut taken = std::collections::HashSet::new();
        let mut apps: Vec<&Arc<AppTools>> = self.apps.iter().collect();
        apps.sort_by(|a, b| a.server.cmp(&b.server));
        for app in apps {
            for tool in app.tools_of() {
                // A name two apps share goes to neither (list_tool_names' rule).
                let name = tool["name"].as_str().unwrap_or("").to_string();
                if taken.insert(name) {
                    out.push(tool);
                }
            }
        }
        Ok(out)
    }

    fn call(&self, caller: &Caller, name: &str, arguments: Value) -> Value {
        for app in &self.apps {
            if let Some(expose) = app.expose(name) {
                return app.call_tool(caller, &expose, &arguments, &self.exponential);
            }
        }
        json!({"content": [{"type": "text", "text": format!("Unknown tool: {name}")}], "isError": true})
    }
}

/// Several providers behind the one hub seat the host gets: a name the
/// first one lists goes there.
pub struct Providers(pub Vec<Arc<dyn ToolProvider>>);

impl ToolProvider for Providers {
    fn tools(&self) -> Result<Vec<Value>, String> {
        let mut out = Vec::new();
        for p in &self.0 {
            out.extend(p.tools()?);
        }
        Ok(out)
    }

    fn call(&self, caller: &Caller, name: &str, arguments: Value) -> Value {
        for p in &self.0 {
            if p.tools().unwrap_or_default().iter().any(|t| t["name"] == name) {
                return p.call(caller, name, arguments);
            }
        }
        json!({"content": [{"type": "text", "text": format!("Unknown tool: {name}")}], "isError": true})
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    use eengine::{FormFactor, NetworkPolicy, RunnerConfig};
    use eengine_runner::runner::{Dispatch, Providers as RunnerProviders, Runner};

    const SAMPLE: &str = include_str!("../../../../../ede/apps/notes.app.json");

    /// Writes straight into the runner (no pump in a test).
    struct Direct(Mutex<Runner>);

    impl Writes for Direct {
        fn upsert(&self, args: &Value) -> Result<Value, String> {
            match self.0.lock().unwrap().dispatch("records_upsert", args) {
                Dispatch::Done(eengine::CallResult::Value(v)) => Ok(v),
                Dispatch::Done(eengine::CallResult::Refused(f)) => Err(f.message),
                _ => Err("not a records call".into()),
            }
        }
    }

    struct Nouns(Mutex<Vec<(String, Value)>>);

    impl ToolProvider for Nouns {
        fn tools(&self) -> Result<Vec<Value>, String> {
            Ok(vec![json!({"name": "exponential_issues"})])
        }
        fn call(&self, _caller: &Caller, name: &str, arguments: Value) -> Value {
            self.0.lock().unwrap().push((name.to_string(), arguments.clone()));
            json!({"content": [{"type": "text", "text": "[]"}], "structuredContent": {"rows": []}})
        }
    }

    fn app() -> (Arc<AppTools>, Arc<Mutex<Runner>>) {
        let dir = tempfile::tempdir().unwrap();
        let file: Value = serde_json::from_str(SAMPLE).unwrap();
        let stores = crate::engine::Stores::open(dir.path().join("data"), Vec::new()).unwrap();
        let providers = RunnerProviders { host_kind: "ede".into(), platform: "linux".into(), locale: "en-US".into(), storage: stores.storage.clone(), blobs: stores.blobs.clone(), vault: Arc::new(eengine::MemoryVault::default()), capabilities: Vec::new() };
        let config = RunnerConfig { id: "at.exponential.notes".into(), name: "Notes".into(), version: "1.0.0".into(), network: NetworkPolicy::Any };
        let mut runner = Runner::boot(config, &file, providers, FormFactor::Wide).expect("boots");
        let _ = runner.launch(Vec::new());
        let package = runner.instance().package().clone();
        let runner = Arc::new(Mutex::new(runner));
        let writes = Arc::new(Direct(Mutex::new(Runner::boot(RunnerConfig { id: "at.exponential.notes".into(), name: "Notes".into(), version: "1.0.0".into(), network: NetworkPolicy::Any }, &file, RunnerProviders { host_kind: "ede".into(), platform: "linux".into(), locale: "en-US".into(), storage: stores.storage.clone(), blobs: stores.blobs.clone(), vault: Arc::new(eengine::MemoryVault::default()), capabilities: Vec::new() }, FormFactor::Wide).unwrap())));
        std::mem::forget(dir);
        (Arc::new(AppTools { id: "at.exponential.notes".into(), server: server_name("at.exponential.notes"), package, storage: stores.storage.clone(), me: "device-1".into(), writes }), runner)
    }

    fn caller() -> Caller {
        Caller { session_id: ede_protocol::SessionId::new("run-1").unwrap(), label: "claude".into() }
    }

    #[test]
    fn server_names_are_hub_names() {
        assert_eq!(server_name("at.exponential.notes"), "at-exponential-notes");
        assert_eq!(server_name("Acme_CRM.v2"), "acme-crm-v2");
        assert_eq!(server_name("a.very.long.package.identifier.that.goes.on.and.on"), "a-very-long-package-identifier-t");
        assert_eq!(server_name("..."), "app");
    }

    #[test]
    fn the_sample_lists_its_read_and_write_tools_with_provenance() {
        let (app, _) = app();
        let provider = AppsProvider { apps: vec![app], exponential: Arc::new(Nouns(Mutex::new(Vec::new()))) };
        let tools = provider.tools().unwrap();
        let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
        assert_eq!(names, ["at-exponential-notes__list_notes", "at-exponential-notes__find_notes", "at-exponential-notes__add_note"]);
        assert_eq!(tools[0]["annotations"]["readOnlyHint"], json!(true));
        assert_eq!(tools[2]["annotations"]["readOnlyHint"], json!(false));
        assert_eq!(tools[0]["_meta"]["at.exponential/app"], json!("at-exponential-notes"));
        assert_eq!(tools[1]["inputSchema"]["required"], json!(["text"]));
    }

    #[test]
    fn a_records_upsert_then_list_round_trip() {
        let (app, _) = app();
        let provider = AppsProvider { apps: vec![app], exponential: Arc::new(Nouns(Mutex::new(Vec::new()))) };
        let missing = provider.call(&caller(), "at-exponential-notes__add_note", json!({}));
        assert_eq!(missing["isError"], json!(true));
        assert!(missing["content"][0]["text"].as_str().unwrap().starts_with("invalidArguments: text is required"));
        let added = provider.call(&caller(), "at-exponential-notes__add_note", json!({"text": "Buy milk"}));
        assert_eq!(added["structuredContent"]["created"], json!(true), "{added}");
        let id = added["structuredContent"]["id"].as_str().unwrap().to_string();
        assert!(is_uuid(&id));
        let listed = provider.call(&caller(), "at-exponential-notes__list_notes", json!({}));
        assert_eq!(listed["structuredContent"]["count"], json!(1), "{listed}");
        assert_eq!(listed["structuredContent"]["rows"][0]["text"], json!("Buy milk"));
        assert_eq!(listed["_meta"]["at.exponential/tool"], json!("list_notes"));
        let found = provider.call(&caller(), "at-exponential-notes__find_notes", json!({"text": "Buy milk"}));
        assert_eq!(found["structuredContent"]["count"], json!(1), "{found}");
        let none = provider.call(&caller(), "at-exponential-notes__find_notes", json!({"text": "Nothing"}));
        assert_eq!(none["structuredContent"]["count"], json!(0), "{none}");
        assert_eq!(provider.call(&caller(), "nope__x", json!({}))["isError"], json!(true));
    }

    #[test]
    fn an_action_run_calls_the_desktops_noun_via_the_app() {
        let (app, _) = app();
        let nouns = Arc::new(Nouns(Mutex::new(Vec::new())));
        let mut tools = AppTools { id: app.id.clone(), server: app.server.clone(), package: app.package.clone(), storage: app.storage.clone(), me: app.me.clone(), writes: app.writes.clone() };
        tools.package["exposes"].as_array_mut().unwrap().push(json!({"name": "open_issues", "title": "Open issues", "description": "The open issues.", "input": {"type": "object", "properties": {"q": {"type": "string"}}}, "access": "read", "run": {"action": "exponential_issues_list", "args": {"search": "${input.q}"}}}));
        let provider = AppsProvider { apps: vec![Arc::new(tools)], exponential: nouns.clone() };
        let out = provider.call(&caller(), "at-exponential-notes__open_issues", json!({"q": "printer"}));
        assert_eq!(out["_meta"]["at.exponential/via"], json!("action"), "{out}");
        let calls = nouns.0.lock().unwrap();
        assert_eq!(calls[0].0, "exponential_issues");
        assert_eq!(calls[0].1, json!({"op": "list", "search": "printer"}));
    }

    #[test]
    fn several_providers_share_the_hub_seat() {
        let (app, _) = app();
        let p = Providers(vec![Arc::new(crate::engine::shell_edit::ShellEdit::with(std::path::PathBuf::from("/tmp"), Arc::new(|_, _| false), Arc::new(|_, _, _| Ok(())))), Arc::new(AppsProvider { apps: vec![app], exponential: Arc::new(Nouns(Mutex::new(Vec::new()))) })]);
        let names: Vec<String> = p.tools().unwrap().iter().map(|t| t["name"].as_str().unwrap().to_string()).collect();
        assert_eq!(names, ["shell_edit", "at-exponential-notes__list_notes", "at-exponential-notes__find_notes", "at-exponential-notes__add_note"]);
        assert_eq!(p.call(&caller(), "at-exponential-notes__list_notes", json!({}))["structuredContent"]["count"], json!(0));
    }
}
