//! The runner's core, painter-free: ONE Engine `AppInstance` over the
//! bundle, and what each thing that happens to the window means for it, as
//! [`Paint`]s the painter carries out (install the shell package, receive a
//! genUI message, pause or resume the sources, settle a function call).
//! The gpui half (`main.rs`) only turns window events into these calls.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use eengine::broker::{GrantValue, Verdict};
use eengine::capability::Broker;
use eengine::consent::{ConsentAnswer, GrantVia};
use eengine::providers::SystemClock;
use eengine::providers::{engine_record_refusal, engine_viewer, Failure, Identity, RecordEvent, Subscription, WriteAck, WriteBatch};
use eengine::outbox::{outbox_writes, OutboxEntry, OutboxWrite};
use eengine::synced::{record_call, SyncedRecordStore};
use eengine::{
    runner_failure, runner_package, runner_toast, AppInstance, BlobStore, CallResult, Capability, Effect, FormFactor, HostConfig, HostInfo, LifecycleEvent, LoadError, Presentation, RecordStore, RunnerConfig, RunnerFailure, Vault, WindowHost,
};
use serde_json::{json, Map, Value};

use crate::events::{refused_link_copy, RunnerEvent};

/// The surfaces a runner layers, bottom to top (runner.json SURFACES).
pub const APP: &str = "app";
pub const CONSENT: &str = "consent";
pub const PERMISSIONS: &str = "permissions";
pub const TOAST: &str = "toast";
pub const SURFACES: [&str; 4] = [APP, CONSENT, PERMISSIONS, TOAST];

const VERSION: &str = eui_core::catalog::A2UI_VERSION;

/// What the painter does.
#[derive(Clone, Debug, PartialEq)]
pub enum Paint {
    /// Install this (the shell-wrapped) package in the genUI host.
    Install(Value),
    /// One genUI server message (`applyTemplate`, `createSurface`, …).
    Message(Value),
    /// Live sources on or off (lifecycle.json `sources`).
    Live(bool),
    /// The answer of a function call that waited for the consent sheet.
    Settle(u64, Result<Value, String>),
}

/// A surface made of `components` (+ its data), replacing any earlier one.
pub fn surface_messages(surface_id: &str, components: &[Value], data: Option<&Value>) -> Vec<Value> {
    let catalog = json!(eui_core::catalog::CORE_CATALOG_ID);
    let mut out = vec![
        json!({"version": VERSION, "createSurface": {"surfaceId": surface_id, "catalogId": catalog}}),
        json!({"version": VERSION, "updateComponents": {"surfaceId": surface_id, "components": components}}),
    ];
    if let Some(data) = data {
        out.push(json!({"version": VERSION, "updateDataModel": {"surfaceId": surface_id, "path": "/", "value": data}}));
    }
    out
}

/// Round 9 §1.4: surface ids are unique for the router's lifetime, so a
/// runner surface painted again gets a minted id; `SURFACES` stay the names
/// the runner speaks. [`Self::rewrite`] maps a message onto the live id.
#[derive(Default)]
pub struct LiveSurfaces {
    live: HashMap<String, String>,
    created: HashSet<String>,
}

impl LiveSurfaces {
    /// The live id behind a runner surface.
    pub fn live<'a>(&'a self, surface: &'a str) -> &'a str {
        self.live.get(surface).map_or(surface, String::as_str)
    }

    /// `message` on the live id; a surface opened again = a minted id, and
    /// the second message (delete the old one) runs right after it.
    pub fn rewrite(&mut self, message: &Value, mint: impl FnOnce(&str) -> String) -> (Value, Option<Value>) {
        let Some((kind, id)) = surface_of(message) else { return (message.clone(), None) };
        let (target, replaced) = match kind.as_str() {
            "createSurface" | "applyTemplate" => {
                if self.created.insert(id.clone()) {
                    self.live.insert(id.clone(), id.clone());
                    (id, None)
                } else {
                    let minted = mint(&id);
                    let old = self.live.insert(id, minted.clone());
                    (minted, old)
                }
            }
            "deleteSurface" => (self.live.remove(&id).unwrap_or(id), None),
            _ => (self.live(&id).to_string(), None),
        };
        let mut out = message.clone();
        out[kind.as_str()]["surfaceId"] = Value::String(target);
        (out, replaced.map(|old| delete_message(&old)))
    }
}

fn surface_of(message: &Value) -> Option<(String, String)> {
    let (kind, body) = message.as_object()?.iter().find(|(k, _)| *k != "version")?;
    Some((kind.clone(), body.get("surfaceId")?.as_str()?.to_string()))
}

pub fn delete_message(surface_id: &str) -> Value {
    json!({"version": VERSION, "deleteSurface": {"surfaceId": surface_id}})
}

/// The WindowHost `open`: template `screen` on surface `app` with data `{params}`.
pub fn open_message(package_id: &str, screen: &str, params: &Map<String, Value>) -> Value {
    json!({"version": VERSION, "applyTemplate": {"surfaceId": APP, "templateId": screen, "packageId": package_id, "data": {"params": params}}})
}

/// What the runner paints in place of the app when its bundle cannot load.
pub fn failure_paints(kind: RunnerFailure) -> Vec<Paint> {
    surface_messages(APP, &runner_failure(kind), None).into_iter().map(Paint::Message).collect()
}

/// The failed-action toast (surface `toast`).
pub fn toast_paints(message: &str) -> Vec<Paint> {
    surface_messages(TOAST, &runner_toast(message), None).into_iter().map(Paint::Message).collect()
}

/// The WindowHost: the Engine's opens queue here and the runner paints them
/// after each call (the instance is busy while it calls `open`).
#[derive(Default)]
pub struct Opens(Mutex<Vec<(String, Map<String, Value>)>>);

impl WindowHost for Opens {
    fn open(&self, _app: &str, screen: &str, params: &Map<String, Value>, _how: Presentation) {
        // Desktop: every presentation is the one window's `app` surface.
        self.0.lock().unwrap_or_else(|p| p.into_inner()).push((screen.to_string(), params.clone()));
    }
}

impl Opens {
    fn drain(&self) -> Vec<(String, Map<String, Value>)> {
        std::mem::take(&mut *self.0.lock().unwrap_or_else(|p| p.into_inner()))
    }
}

/// The providers a platform hands in (the gpui half: SQLite, files, the keystore, the E5 natives).
pub struct Providers {
    /// providers.json `hosts`: `exported` for a bundle, `ede` for Exponential Desktop's shell (#VAPP-273).
    pub host_kind: String,
    pub platform: String,
    pub locale: String,
    pub storage: Arc<dyn RecordStore>,
    pub blobs: Arc<dyn BlobStore>,
    pub vault: Arc<dyn Vault>,
    pub capabilities: Vec<Arc<dyn Capability>>,
    /// #VAPP-273: the viewer; `None` = this device (identity `local`).
    pub identity: Option<Arc<dyn Identity>>,
    /// #VAPP-273: a connected host's synced store: the app's rows are its
    /// visible rows (the synced cache with the outbox over it) and every
    /// `records_*` call goes through its outbox, never `storage.write`.
    pub synced: Option<Synced>,
}

/// A connected host's synced store for one app (`app` = the app's key in
/// that store, the server's app id).
#[derive(Clone)]
pub struct Synced {
    pub store: Arc<SyncedRecordStore>,
    pub app: String,
    /// The host's view of the connection (an offline write queues).
    pub online: Arc<std::sync::atomic::AtomicBool>,
}

/// The synced store as the instance reads it: the visible rows of one app.
struct Overlay {
    synced: Synced,
    me: String,
}

impl RecordStore for Overlay {
    fn rows(&self, _app: &str) -> Result<Vec<eui_core::host::RecordRow>, Failure> {
        Ok(self.synced.store.visible_rows(&self.synced.app, Some(&self.me)))
    }

    fn write(&self, _app: &str, _batch: &WriteBatch, _by: Option<&str>) -> Result<WriteAck, Failure> {
        Err(Failure::new("BAD_REQUEST", "a synced store writes through its outbox"))
    }

    fn history(&self, _app: &str, _id: &str) -> Result<Vec<RecordEvent>, Failure> {
        Ok(Vec::new())
    }

    fn subscribe(&self, _app: &str, on: Box<dyn Fn(eui_core::host::RecordsDelta) + Send + Sync>) -> Subscription {
        self.synced.store.subscribe(&self.synced.app, on)
    }
}

/// A function call that waits for the person (`CallResult::Ask`).
#[derive(Clone, Debug, PartialEq)]
pub struct PendingAsk {
    pub ticket: u64,
    pub name: String,
    pub args: Value,
    pub verdict: Verdict,
}

/// Where a function call goes.
pub enum Dispatch {
    /// Answered here (records, navigation).
    Done(CallResult),
    /// A capability: [`Broker::call`] on a worker thread (its natives block
    /// on dialogs), then [`Runner::finish`] with the answer.
    Broker(Arc<Broker>),
    /// #VAPP-273: a records write of a connected host: `store.submit(entry,
    /// online)` on a worker thread (it may wait for the server), then
    /// [`Runner::finish`]: `Done` = its answer, `Queued` = the entry's,
    /// `Refused` = the server's refusal.
    Synced { synced: Synced, entry: OutboxEntry },
}

pub struct Runner {
    instance: AppInstance,
    opens: Arc<Opens>,
    storage: Arc<dyn RecordStore>,
    blobs: Arc<dyn BlobStore>,
    config: RunnerConfig,
    form: FormFactor,
    pending: Option<PendingAsk>,
    identity: Arc<dyn Identity>,
    synced: Option<Synced>,
}

impl Runner {
    /// One Engine instance over the bundle's package file: identity `local`,
    /// the providers' host kind, network = the config's. `signature` / `invalid` = the failure to paint.
    pub fn boot(config: RunnerConfig, file: &Value, providers: Providers, form: FormFactor) -> Result<Runner, RunnerFailure> {
        let opens = Arc::new(Opens::default());
        let device = crate::vault::device_id(providers.vault.as_ref());
        let identity: Arc<dyn Identity> = providers.identity.clone().unwrap_or_else(|| Arc::new(eengine::LocalIdentity { device_id: device }));
        let me = engine_viewer(&identity.identity()).id;
        // A connected host: the instance reads the synced store's visible rows.
        let storage: Arc<dyn RecordStore> = match &providers.synced {
            Some(synced) => Arc::new(Overlay { synced: synced.clone(), me }),
            None => providers.storage.clone(),
        };
        let host = HostInfo { kind: providers.host_kind, platform: providers.platform, form_factor: form_name(form).into(), locale: providers.locale };
        let host_config = HostConfig {
            host,
            storage: storage.clone(),
            blobs: providers.blobs.clone(),
            identity: identity.clone(),
            vault: providers.vault,
            window: opens.clone(),
            clock: Arc::new(SystemClock),
            zone: None,
            network: config.network,
            sync: None,
            capabilities: providers.capabilities,
            // The runner paints the sheet itself (surface `consent`).
            consent: None,
            audit: None,
        };
        let instance = AppInstance::load(host_config, file).map_err(|e| match e {
            LoadError::Signature(_) => RunnerFailure::Signature,
            LoadError::Invalid(_) => RunnerFailure::Invalid,
        })?;
        Ok(Runner { instance, opens, storage, blobs: providers.blobs, config, form, pending: None, identity, synced: providers.synced })
    }

    pub fn config(&self) -> &RunnerConfig {
        &self.config
    }

    pub fn app_id(&self) -> &str {
        self.instance.app_id()
    }

    pub fn instance(&self) -> &AppInstance {
        &self.instance
    }

    pub fn storage(&self) -> &Arc<dyn RecordStore> {
        &self.storage
    }

    /// VAPP-271 (`catalog/host.json` media.appFile): the app's own files for
    /// an `app-file:<fileId>` src, read from this instance's BlobStore under
    /// its app id (the media loader asks off the main thread; a missing
    /// file or a store failure = `None`, the fallback paints).
    pub fn app_files(&self) -> Arc<dyn eui_gpui::host::AppFiles> {
        let blobs = self.blobs.clone();
        let app = self.app_id().to_string();
        Arc::new(move |file_id: &str| blobs.get(&app, file_id).ok().flatten())
    }

    pub fn form(&self) -> FormFactor {
        self.form
    }

    pub fn pending(&self) -> Option<&PendingAsk> {
        self.pending.as_ref()
    }

    /// The package the painter installs for the current form factor.
    pub fn shell_package(&self) -> Value {
        runner_package(self.instance.package(), self.form)
    }

    fn opened(&self) -> Vec<Paint> {
        let id = self.instance.app_id().to_string();
        self.opens.drain().into_iter().map(|(screen, params)| Paint::Message(open_message(&id, &screen, &params))).collect()
    }

    /// Launch: the shell package, then `launch` (or the first deep link,
    /// which launches cold), then the other links. A plain launch that opens
    /// nothing = the derived package's first screen.
    pub fn launch(&mut self, links: Vec<LifecycleEvent>) -> Vec<Paint> {
        let mut out = vec![Paint::Install(self.shell_package())];
        let mut links = links.into_iter();
        let first = links.next().unwrap_or(LifecycleEvent::Launch);
        out.extend(self.lifecycle(&first));
        for link in links {
            out.extend(self.lifecycle(&link));
        }
        if self.instance.state().screen.is_none() {
            let shell = self.shell_package();
            if let Some(first) = eui_core::host::package_screens(&shell).first().and_then(|s| s.get("template")).and_then(Value::as_str) {
                out.push(Paint::Message(open_message(self.instance.app_id(), first, &Map::new())));
            }
        }
        out
    }

    /// One lifecycle event and its effects (the instance carries out open,
    /// persist and flush; the painter the sources; a refusal is the toast).
    pub fn lifecycle(&mut self, event: &LifecycleEvent) -> Vec<Paint> {
        let effects = self.instance.event(event);
        let mut out = self.opened();
        for effect in effects {
            match effect {
                Effect::Sources { live } => out.push(Paint::Live(live)),
                Effect::Refused { .. } => out.extend(toast_paints(&refused_link_copy())),
                _ => {}
            }
        }
        out
    }

    /// The window's width changed: crossing `md` re-installs the shell and re-applies the open screen.
    pub fn resize(&mut self, width: f32) -> Vec<Paint> {
        let form = crate::events::form_factor(width);
        if form == self.form {
            return Vec::new();
        }
        self.form = form;
        let mut out = vec![Paint::Install(self.shell_package())];
        let state = self.instance.state();
        let screen = state.screen.clone().or_else(|| eui_core::host::package_screens(&self.shell_package()).first().and_then(|s| s.get("template")).and_then(Value::as_str).map(str::to_string));
        if let Some(screen) = screen {
            out.push(Paint::Message(open_message(self.instance.app_id(), &screen, &state.params.clone().unwrap_or_default())));
        }
        out
    }

    fn permissions(&self) -> Vec<Paint> {
        surface_messages(PERMISSIONS, &self.instance.permissions_sheet(), None).into_iter().map(Paint::Message).collect()
    }

    /// An event the runner handles itself. `ConsentAnswer` answers `Some`
    /// with the pending ask and the answer: [`Broker::answer`] it on a
    /// worker, then [`Runner::finish`].
    pub fn runner_event(&mut self, event: RunnerEvent) -> (Vec<Paint>, Option<(PendingAsk, ConsentAnswer)>) {
        match event {
            RunnerEvent::Navigate { screen } => {
                let result = self.instance.call("app_navigate", &json!({"screen": screen}), &[]);
                let mut out = self.opened();
                if let CallResult::Refused(f) = result {
                    out.extend(toast_paints(&f.message));
                }
                (out, None)
            }
            RunnerEvent::Permissions => (self.permissions(), None),
            RunnerEvent::ConsentToggle { scope } => {
                let value = match self.instance.grants().get(&scope) {
                    Some(GrantValue::Granted) => None,
                    _ => Some(GrantValue::Granted),
                };
                let mut out = Vec::new();
                if let Err(reason) = self.instance.set_grant(GrantVia::ConsentUi, true, &scope, value) {
                    out.extend(toast_paints(reason));
                }
                out.extend(self.permissions());
                (out, None)
            }
            RunnerEvent::ConsentAnswer { answer } => {
                let out = vec![Paint::Message(delete_message(CONSENT))];
                (out, self.pending.take().map(|p| (p, answer)))
            }
        }
    }

    /// Where an app's function call goes: `records_*` and `app_navigate`
    /// here (fresh uuids for the creates), a capability to the broker.
    pub fn dispatch(&mut self, name: &str, args: &Value) -> Dispatch {
        if let (Some(synced), Some(call)) = (&self.synced, record_call(name)) {
            // A connected host: the kind policy as the viewer, then the outbox.
            let entry = synced.store.entry(&synced.app, call, args);
            if let Some(refusal) = self.refusal(&entry.writes) {
                return Dispatch::Done(CallResult::Refused(Failure::new("FORBIDDEN", refusal)));
            }
            return Dispatch::Synced { synced: synced.clone(), entry };
        }
        if name.starts_with("records_") || name == "app_navigate" {
            Dispatch::Done(self.instance.call(name, args, &new_ids(name, args)))
        } else {
            Dispatch::Broker(self.instance.broker().clone())
        }
    }

    /// The kind policy check of a synced write, as the instance does for a
    /// direct one (`engine_record_refusal` over the package's policies).
    fn refusal(&self, writes: &[OutboxWrite]) -> Option<String> {
        let identity = self.identity.identity();
        let rows = self.storage.rows(self.instance.app_id()).unwrap_or_default();
        let pkg = self.instance.package();
        for w in writes {
            let (kind, op, row) = match w {
                OutboxWrite::Create { kind, .. } => (Some(kind.clone()), eui_core::host::RecordOp::Create, None),
                OutboxWrite::Update { id, kind, .. } => (Some(kind.clone()), eui_core::host::RecordOp::Update, rows.iter().find(|r| r.id == *id)),
                OutboxWrite::Delete { id } => {
                    let row = rows.iter().find(|r| r.id == *id);
                    (row.map(|r| r.kind.clone()), eui_core::host::RecordOp::Delete, row)
                }
            };
            let def = kind.as_deref().and_then(|k| pkg.get("records").and_then(|r| r.get(k)));
            let policy = eui_core::host::record_policy(def);
            let row = row.map(|r| json!({"authorId": r.author_id, "createdBy": r.created_by}));
            if let Some(refusal) = engine_record_refusal(&identity, &policy, op, row.as_ref()) {
                return Some(refusal.to_string());
            }
        }
        let _ = outbox_writes;
        None
    }

    /// A call's answer → its result for genUI (`None` = it waits for the
    /// consent sheet, settled later as [`Paint::Settle`]) and what to paint.
    pub fn finish(&mut self, ticket: u64, name: &str, args: &Value, result: CallResult) -> (Option<Result<Value, String>>, Vec<Paint>) {
        let mut out = self.opened();
        let answer = match result {
            CallResult::Value(value) => Some(Ok(value)),
            CallResult::Refused(f) => {
                out.extend(toast_paints(&f.message));
                Some(Err(f.message))
            }
            CallResult::Ask(_, _) if self.pending.is_some() => Some(Err("consentPending".into())),
            CallResult::Ask(verdict, sheet) => {
                self.pending = Some(PendingAsk { ticket, name: name.into(), args: args.clone(), verdict });
                out.extend(surface_messages(CONSENT, &sheet.components, Some(&sheet.data)).into_iter().map(Paint::Message));
                None
            }
        };
        (answer, out)
    }

    /// A `records:` source's value.
    pub fn records(&self, uri: &str) -> Option<Value> {
        self.instance.records_value(uri)
    }

    /// The Engine's last state, for `terminate`.
    pub fn terminate(&mut self) -> Vec<Paint> {
        self.lifecycle(&LifecycleEvent::Terminate)
    }
}

fn form_name(form: FormFactor) -> &'static str {
    match form {
        FormFactor::Wide => "wide",
        FormFactor::Compact => "compact",
    }
}

/// One uuid per write a call may create (the Engine takes them in order).
pub fn new_ids(name: &str, args: &Value) -> Vec<String> {
    let count = match name {
        "records_upsert" => usize::from(args.get("id").is_none()),
        "records_batch" => args.get("writes").and_then(Value::as_array).map_or(0, |w| w.iter().filter(|w| w.get("id").is_none()).count()),
        _ => 0,
    };
    (0..count).map(|_| uuid::Uuid::new_v4().to_string()).collect()
}

/// The function names the painter registers for a package: the records
/// calls and every app-facing tool of each declared capability.
pub fn function_names(pkg: &Value) -> Vec<String> {
    let mut names: Vec<String> = ["records_upsert", "records_delete", "records_batch"].map(String::from).to_vec();
    for entry in eengine::ManifestEntry::of_package(pkg) {
        if let Some((cap, scope)) = eengine::broker::scope_def(&entry.id) {
            for tool in &scope.tools {
                let name = format!("{}_{tool}", cap.name);
                if !names.contains(&name) {
                    names.push(name);
                }
            }
        }
    }
    names
}

/// The broker's answer as a `CallResult` (what `AppInstance::call` answers,
/// VAPP-271): the app reads a capability's `structuredContent` (an action
/// step's `results.<name>`); an `isError` result is a refusal, its text the toast.
pub fn brokered(answer: eengine::BrokerAnswer) -> CallResult {
    // VAPP-271: the Engine's mapping (an action's `results.<name>` = the
    // answer's `structuredContent`, `isError` = a refusal), never the raw MCP result.
    eengine::brokered(answer)
}

#[cfg(test)]
mod tests;

/// #VAPP-273: a synced write's outcome as the call's result: the server's
/// answer, the entry's own when it queued, the refusal's code and words.
pub fn submitted(synced: &Synced, entry: OutboxEntry) -> CallResult {
    let answer = entry.answer();
    match synced.store.submit(entry, synced.online.load(std::sync::atomic::Ordering::SeqCst)) {
        Ok(eengine::synced::Submitted::Done(value)) => CallResult::Value(value),
        Ok(eengine::synced::Submitted::Queued) => CallResult::Value(answer),
        Ok(eengine::synced::Submitted::Refused(pushed)) => {
            let code = match &pushed.result {
                eengine::outbox::OutboxResult::Code(code) => code.clone(),
                _ => "UNAVAILABLE".to_string(),
            };
            CallResult::Refused(Failure::new(code, pushed.message.unwrap_or_else(|| "the server refused the write".into())))
        }
        Err(failure) => CallResult::Refused(failure),
    }
}


