//! #VAPP-263: the UniFFI facade over the Engine (`eengine`) for Swift (iOS,
//! macOS) and Kotlin (Android), coarse like genUI's `exponential-ui-ffi`.
//! Swift and Kotlin never re-implement the Engine: they hand it their
//! providers and keep the painters (`docs/spec/apps.md` §13.1).
//!
//! Shape of the contract:
//! - `Engine(host, identity, providers)`: one per host process. `load(file)`
//!   verifies and validates a package file (or a bare package) and answers
//!   an `AppInstance`.
//! - `AppInstance`: `event(lifecycle event)` → effects (open/persist/flush
//!   are carried out through the providers), `recordsValue(source)`,
//!   `call(name, args, newIds)` → `CallResult` (a value, a consent sheet to
//!   paint, or a refusal), `answerConsent`, `setGrant` (the Permissions
//!   sheet's Switch), `permissionsSheet()`, the outbox. With `hub`:
//!   `agentAccess(run)` serves the app's capabilities to an on-device agent.
//! - The providers are foreign traits Swift/Kotlin implement: `RecordStore`
//!   (optional: the default is the SQLite store under `dataDir`, feature
//!   `sqlite`), `BlobStore` (optional: files under `dataDir`), `Vault`
//!   (Keychain, Keystore), `WindowHost`, `Capability` (native tools),
//!   `ConsentUi` (optional: answer the sheet inline), `SyncTransport`
//!   (optional: queue writes in the outbox), `CallAudit`, `EngineClock`.
//! - JSON text crosses where the Engine's own types are JSON (packages,
//!   rows, events, effects, MCP tools and results, genUI surfaces), like
//!   genUI's facade; the fixtures under `packages/eengine/fixtures` spell them.
//! - A callback runs on the caller's thread while the instance is locked:
//!   a provider must not call back into the same `AppInstance` synchronously.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use serde_json::{Map, Value};

use eengine::broker::Verdict;
use eengine::consent::{ConsentAnswer as CoreAnswer, GrantVia};
use eengine::instance::{CallResult as CoreCall, LoadError};
use eengine::lifecycle::LifecycleEvent;
use eengine::outbox::{OutboxMode, OutboxResult};
use eengine::providers as core;

uniffi::setup_scaffolding!();

// --- errors ------------------------------------------------------------

#[derive(Debug, uniffi::Error)]
pub enum EngineError {
    /// Bad input from the host (JSON that does not parse, a config problem).
    Invalid { reason: String },
    /// The package did not load: `kind` = `signature` | `invalid`,
    /// `issuesJson` = `[{path, message}]`.
    Load { kind: String, issues_json: String },
    /// A provider's refusal or failure (eCore's shape).
    Failure { code: String, reason: String },
}

impl std::error::Error for EngineError {}

impl std::fmt::Display for EngineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EngineError::Invalid { reason } => write!(f, "invalid: {reason}"),
            EngineError::Load { kind, issues_json } => write!(f, "load ({kind}): {issues_json}"),
            EngineError::Failure { code, reason } => write!(f, "{code}: {reason}"),
        }
    }
}

impl From<uniffi::UnexpectedUniFFICallbackError> for EngineError {
    fn from(e: uniffi::UnexpectedUniFFICallbackError) -> Self {
        EngineError::Failure { code: "INTERNAL".into(), reason: e.reason }
    }
}

impl From<EngineError> for core::Failure {
    fn from(e: EngineError) -> Self {
        match e {
            EngineError::Failure { code, reason } => core::Failure::new(code, reason),
            other => core::Failure::new("INTERNAL", other.to_string()),
        }
    }
}

fn invalid(reason: impl Into<String>) -> EngineError {
    EngineError::Invalid { reason: reason.into() }
}

fn parse(what: &str, json: &str) -> Result<Value, EngineError> {
    serde_json::from_str(json).map_err(|e| invalid(format!("{what}: {e}")))
}

fn text(value: &impl serde::Serialize) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "null".into())
}

// --- the providers (foreign traits) -------------------------------------

/// Where an app's records live; one store serves every app of a host.
/// Rows, batches and events are the Engine's JSON (`RecordRow`, `WriteBatch`,
/// `WriteAck`, `RecordEvent`).
#[uniffi::export(with_foreign)]
pub trait RecordStore: Send + Sync {
    /// Every row of `app`: `[{id, kind, data, createdBy?, authorId?, createdAt, updatedAt}]`.
    fn rows(&self, app: String) -> Result<String, EngineError>;
    /// Apply one batch (`{writeId, writes, at}`), all or nothing; a known
    /// `writeId` answers its first ack. Answers `{ids, replayed}`.
    fn write(&self, app: String, batch_json: String, by: Option<String>) -> Result<String, EngineError>;
    /// A record's events, oldest first.
    fn history(&self, app: String, id: String) -> Result<String, EngineError>;
    /// Call `listener.changed({upserts, deletes})` on every change to `app`'s
    /// rows; answers a token for `unsubscribe`.
    fn subscribe(&self, app: String, listener: Arc<RecordsListener>) -> u64;
    fn unsubscribe(&self, token: u64);
}

/// Record files (format `attachment`).
#[uniffi::export(with_foreign)]
pub trait BlobStore: Send + Sync {
    /// Store bytes; answers the file id.
    fn put(&self, app: String, name: String, mime: String, bytes: Vec<u8>) -> Result<String, EngineError>;
    fn get(&self, app: String, id: String) -> Result<Option<Vec<u8>>, EngineError>;
    fn delete(&self, app: String, id: String) -> Result<(), EngineError>;
}

/// Where secrets, grants and the last screen live (Keychain, Keystore).
#[uniffi::export(with_foreign)]
pub trait Vault: Send + Sync {
    fn read(&self, key: String) -> Option<String>;
    fn write(&self, key: String, value: String) -> Result<(), EngineError>;
    fn delete(&self, key: String) -> Result<(), EngineError>;
}

/// How a host presents a screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum Presentation {
    Replace,
    Push,
    Sheet,
}

/// Maps screens to windows (desktop) or nav stacks and sheets (phones).
#[uniffi::export(with_foreign)]
pub trait WindowHost: Send + Sync {
    /// Show `screen` of `app` (`params` = a JSON object).
    fn present(&self, app: String, screen: String, params_json: String, how: Presentation);
}

/// A native tool provider mounted under its catalog name (`photos`):
/// MCP tool objects in, MCP call results out.
#[uniffi::export(with_foreign)]
pub trait Capability: Send + Sync {
    fn name(&self) -> String;
    /// Its tools as a JSON array of MCP tool objects.
    fn tools_json(&self) -> String;
    /// One call; answers an MCP call result (`{content, isError?}`).
    fn call(&self, tool: String, arguments_json: String) -> Result<String, EngineError>;
}

/// The person's answer to the consent sheet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum ConsentAnswer {
    Allow,
    Once,
    Deny,
    Dismiss,
}

impl From<ConsentAnswer> for CoreAnswer {
    fn from(answer: ConsentAnswer) -> Self {
        match answer {
            ConsentAnswer::Allow => CoreAnswer::Allow,
            ConsentAnswer::Once => CoreAnswer::Once,
            ConsentAnswer::Deny => CoreAnswer::Deny,
            ConsentAnswer::Dismiss => CoreAnswer::Dismiss,
        }
    }
}

/// The ask sheet (consent.json `consentSheet`): a genUI surface.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct ConsentSheet {
    pub scope: String,
    /// The surface's components (JSON array).
    pub components_json: String,
    /// Its data model (JSON object).
    pub data_json: String,
}

impl From<&eengine::ConsentSheet> for ConsentSheet {
    fn from(sheet: &eengine::ConsentSheet) -> Self {
        ConsentSheet { scope: sheet.scope.clone(), components_json: text(&sheet.components), data_json: text(&sheet.data) }
    }
}

/// A host that answers the sheet inline (it blocks for the answer). Without
/// one, `call` answers `CallResult.ask` and the host answers later.
#[uniffi::export(with_foreign)]
pub trait ConsentUi: Send + Sync {
    fn ask(&self, app: String, sheet: ConsentSheet) -> ConsentAnswer;
}

/// An optional transport to an authority (Exponential: tRPC out). With one,
/// every app write queues in the outbox; without one, writes go to the store.
#[uniffi::export(with_foreign)]
pub trait SyncTransport: Send + Sync {
    /// Replay one outbox entry; answers `{"ok":true,"answer":…}`,
    /// `{"transport":true}` or `{"code":"…","message":"…","invalid":true}`
    /// (`answer`, `message` and `invalid` are optional).
    fn push(&self, app: String, call: String, input_json: String) -> String;
}

/// Where an account's outbox persists (GRDB, Room), oldest entry first.
#[uniffi::export(with_foreign)]
pub trait OutboxLog: Send + Sync {
    /// The entries as a JSON array (a row the Engine cannot read is skipped).
    fn entries_json(&self) -> Result<String, EngineError>;
    fn push(&self, entry_json: String) -> Result<(), EngineError>;
    fn ack(&self, write_id: String, at: String) -> Result<(), EngineError>;
    fn remove(&self, write_ids: Vec<String>) -> Result<(), EngineError>;
}

/// What the host hears from a [`SyncedRecordStore`], on the replaying thread.
#[uniffi::export(with_foreign)]
pub trait SyncedListener: Send + Sync {
    /// The entries moved.
    fn changed(&self);
    /// A replay nobody awaits was refused: the failed-action toast.
    fn refused(&self, entry_json: String, verb: String, message: Option<String>);
}

/// One broker audit row (audit.json's shape).
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct CallAuditRow {
    pub app_id: String,
    pub client_id: String,
    pub tool: String,
    pub status: String,
    pub error: Option<String>,
    pub args_hash: String,
}

/// Where the broker's audit rows go.
#[uniffi::export(with_foreign)]
pub trait CallAudit: Send + Sync {
    fn record(&self, row: CallAuditRow);
}

/// The clock (tests pin it); absent = the system clock.
#[uniffi::export(with_foreign)]
pub trait EngineClock: Send + Sync {
    /// ISO 8601 UTC with milliseconds.
    fn now(&self) -> String;
}

/// A store's change listener (the Engine's side of `RecordStore.subscribe`).
#[derive(uniffi::Object)]
pub struct RecordsListener {
    on: Box<dyn Fn(eui_core::host::RecordsDelta) + Send + Sync>,
}

#[uniffi::export]
impl RecordsListener {
    /// `{upserts: [row], deletes: [id]}`; a delta that does not parse is dropped.
    pub fn changed(&self, delta_json: String) {
        if let Ok(delta) = serde_json::from_str(&delta_json) {
            (self.on)(delta);
        }
    }
}

// --- adapters onto the Engine's traits ----------------------------------

struct Store(Arc<dyn RecordStore>);

impl core::RecordStore for Store {
    fn rows(&self, app: &str) -> Result<Vec<eui_core::host::RecordRow>, core::Failure> {
        let rows = self.0.rows(app.into())?;
        serde_json::from_str(&rows).map_err(|e| core::Failure::new("INTERNAL", format!("rows: {e}")))
    }

    fn write(&self, app: &str, batch: &core::WriteBatch, by: Option<&str>) -> Result<core::WriteAck, core::Failure> {
        let ack = self.0.write(app.into(), text(batch), by.map(str::to_string))?;
        serde_json::from_str(&ack).map_err(|e| core::Failure::new("INTERNAL", format!("ack: {e}")))
    }

    fn history(&self, app: &str, id: &str) -> Result<Vec<core::RecordEvent>, core::Failure> {
        let events = self.0.history(app.into(), id.into())?;
        serde_json::from_str(&events).map_err(|e| core::Failure::new("INTERNAL", format!("history: {e}")))
    }

    fn subscribe(&self, app: &str, on: Box<dyn Fn(eui_core::host::RecordsDelta) + Send + Sync>) -> core::Subscription {
        let token = self.0.subscribe(app.into(), Arc::new(RecordsListener { on }));
        let store = self.0.clone();
        core::Subscription(Some(Box::new(move || store.unsubscribe(token))))
    }
}

struct Blobs(Arc<dyn BlobStore>);

impl core::BlobStore for Blobs {
    fn put(&self, app: &str, name: &str, mime: &str, bytes: &[u8]) -> Result<String, core::Failure> {
        Ok(self.0.put(app.into(), name.into(), mime.into(), bytes.to_vec())?)
    }

    fn get(&self, app: &str, id: &str) -> Result<Option<Vec<u8>>, core::Failure> {
        Ok(self.0.get(app.into(), id.into())?)
    }

    fn delete(&self, app: &str, id: &str) -> Result<(), core::Failure> {
        Ok(self.0.delete(app.into(), id.into())?)
    }
}

/// The blobs of a host without a `dataDir` (previews, tests).
#[derive(Default)]
struct MemoryBlobs(Mutex<HashMap<(String, String), Vec<u8>>>);

impl core::BlobStore for MemoryBlobs {
    fn put(&self, app: &str, _name: &str, _mime: &str, bytes: &[u8]) -> Result<String, core::Failure> {
        let id = uuid_v4();
        self.0.lock().unwrap().insert((app.into(), id.clone()), bytes.to_vec());
        Ok(id)
    }

    fn get(&self, app: &str, id: &str) -> Result<Option<Vec<u8>>, core::Failure> {
        Ok(self.0.lock().unwrap().get(&(app.to_string(), id.to_string())).cloned())
    }

    fn delete(&self, app: &str, id: &str) -> Result<(), core::Failure> {
        self.0.lock().unwrap().remove(&(app.to_string(), id.to_string()));
        Ok(())
    }
}

fn uuid_v4() -> String {
    uuid::Uuid::new_v4().to_string()
}

struct Secrets(Arc<dyn Vault>);

impl core::Vault for Secrets {
    fn read(&self, key: &str) -> Option<String> {
        self.0.read(key.into())
    }

    fn write(&self, key: &str, value: &str) -> Result<(), String> {
        self.0.write(key.into(), value.into()).map_err(|e| e.to_string())
    }

    fn delete(&self, key: &str) -> Result<(), String> {
        self.0.delete(key.into()).map_err(|e| e.to_string())
    }
}

struct Windows(Arc<dyn WindowHost>);

impl core::WindowHost for Windows {
    fn open(&self, app: &str, screen: &str, params: &Map<String, Value>, how: core::Presentation) {
        let how = match how {
            core::Presentation::Replace => Presentation::Replace,
            core::Presentation::Push => Presentation::Push,
            core::Presentation::Sheet => Presentation::Sheet,
        };
        self.0.present(app.into(), screen.into(), text(params), how);
    }
}

struct Native {
    name: String,
    inner: Arc<dyn Capability>,
}

impl eengine::Capability for Native {
    fn name(&self) -> &str {
        &self.name
    }

    fn tools(&self) -> Vec<Value> {
        serde_json::from_str(&self.inner.tools_json()).unwrap_or_default()
    }

    fn call(&self, tool: &str, arguments: &Value) -> Result<Value, core::Failure> {
        let result = self.inner.call(tool.into(), text(arguments))?;
        serde_json::from_str(&result).map_err(|e| core::Failure::new("INTERNAL", format!("{}.{tool}: {e}", self.name)))
    }
}

struct Consent(Arc<dyn ConsentUi>);

impl eengine::ConsentUi for Consent {
    fn ask(&self, app: &str, sheet: &eengine::ConsentSheet) -> CoreAnswer {
        self.0.ask(app.into(), sheet.into()).into()
    }
}

struct Transport(Arc<dyn SyncTransport>);

impl core::SyncTransport for Transport {
    fn push(&self, app: &str, call: &str, input: &Value) -> OutboxResult {
        self.send(app, call, input).result
    }

    fn send(&self, app: &str, call: &str, input: &Value) -> core::Pushed {
        let reply: Value = serde_json::from_str(&self.0.push(app.into(), call.into(), text(input))).unwrap_or(Value::Null);
        core::Pushed {
            result: OutboxResult::from_value(&reply),
            answer: reply.get("answer").cloned().unwrap_or(Value::Null),
            message: reply.get("message").and_then(Value::as_str).map(str::to_string),
            invalid: reply.get("invalid").and_then(Value::as_bool).unwrap_or(false),
        }
    }
}

struct Log(Arc<dyn OutboxLog>);

impl eengine::OutboxLog for Log {
    fn entries(&self) -> Result<Vec<eengine::outbox::OutboxEntry>, core::Failure> {
        let entries = self.0.entries_json().map_err(core::Failure::from)?;
        let rows: Vec<Value> = serde_json::from_str(&entries).map_err(|e| core::Failure::new("BAD_REQUEST", e.to_string()))?;
        // One unreadable row must not stop the account's queue.
        Ok(rows.into_iter().filter_map(|row| serde_json::from_value(row).ok()).collect())
    }

    fn push(&self, entry: &eengine::outbox::OutboxEntry) -> Result<(), core::Failure> {
        self.0.push(text(entry)).map_err(core::Failure::from)
    }

    fn ack(&self, write_id: &str, at: &str) -> Result<(), core::Failure> {
        self.0.ack(write_id.into(), at.into()).map_err(core::Failure::from)
    }

    fn remove(&self, write_ids: &[String]) -> Result<(), core::Failure> {
        self.0.remove(write_ids.to_vec()).map_err(core::Failure::from)
    }
}

struct Heard(Arc<dyn SyncedListener>);

impl eengine::SyncedListener for Heard {
    fn changed(&self) {
        self.0.changed();
    }

    fn refused(&self, entry: &eengine::outbox::OutboxEntry, verb: &str, pushed: &core::Pushed) {
        self.0.refused(text(entry), verb.into(), pushed.message.clone());
    }
}

struct Audit(Arc<dyn CallAudit>);

impl eengine::CallAudit for Audit {
    fn record(&self, row: eengine::broker::BrokerAuditRow) {
        self.0.record(CallAuditRow { app_id: row.app_id, client_id: row.client_id, tool: row.tool, status: row.status, error: row.error, args_hash: row.args_hash });
    }
}

struct ForeignClock(Arc<dyn EngineClock>);

impl core::Clock for ForeignClock {
    fn now(&self) -> String {
        self.0.now()
    }
}

struct Who(core::EngineIdentity);

impl core::Identity for Who {
    fn identity(&self) -> core::EngineIdentity {
        self.0.clone()
    }
}

struct Zone(String);

impl core::HostZone for Zone {
    fn zone(&self) -> String {
        self.0.clone()
    }
}

// --- the host config ----------------------------------------------------

/// What an instance may reach over the network.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum NetworkPolicy {
    Off,
    UpstreamsOnly,
    Any,
}

/// The host (providers.json `hosts`) and where its local data lives.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct EngineHost {
    /// `exponential`, `softcoded`, `ede`, `spezl`, `exported`.
    pub kind: String,
    pub platform: String,
    pub form_factor: String,
    pub locale: String,
    /// The IANA zone today/now/month are read in; None = UTC.
    pub zone: Option<String>,
    pub network: NetworkPolicy,
    /// The default stores' directory (`records.sqlite`, `blobs/`); None =
    /// in memory (previews, tests).
    pub data_dir: Option<String>,
}

/// Who is looking (providers.json `identityKinds`).
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum EngineIdentity {
    /// An Exponential member.
    Member { user_id: String, role: Option<String>, access: Option<String> },
    /// One person on one device, no account: the device uuid owns everything.
    Local { device_id: String },
    /// A P2P key (Spezl).
    Peer { key: String },
}

impl From<EngineIdentity> for core::EngineIdentity {
    fn from(identity: EngineIdentity) -> Self {
        match identity {
            EngineIdentity::Member { user_id, role, access } => core::EngineIdentity::Member { user_id, role, access },
            EngineIdentity::Local { device_id } => core::EngineIdentity::Local { device_id },
            EngineIdentity::Peer { key } => core::EngineIdentity::Peer { key },
        }
    }
}

/// The host's providers. `vault` and `window` are required; every other one
/// has a default or is optional.
#[derive(uniffi::Record)]
pub struct EngineProviders {
    pub vault: Arc<dyn Vault>,
    pub window: Arc<dyn WindowHost>,
    /// None = the SQLite store at `<dataDir>/records.sqlite` (feature
    /// `sqlite`), or an in-memory store without a `dataDir`.
    #[uniffi(default = None)]
    pub storage: Option<Arc<dyn RecordStore>>,
    /// None = files under `<dataDir>/blobs`, or in memory without a `dataDir`.
    #[uniffi(default = None)]
    pub blobs: Option<Arc<dyn BlobStore>>,
    /// Some = app writes queue in the outbox and replay through it.
    #[uniffi(default = None)]
    pub sync: Option<Arc<dyn SyncTransport>>,
    #[uniffi(default = [])]
    pub capabilities: Vec<Arc<dyn Capability>>,
    /// Some = the sheet is answered inline; None = `CallResult.ask`.
    #[uniffi(default = None)]
    pub consent: Option<Arc<dyn ConsentUi>>,
    #[uniffi(default = None)]
    pub audit: Option<Arc<dyn CallAudit>>,
    /// None = the system clock.
    #[uniffi(default = None)]
    pub clock: Option<Arc<dyn EngineClock>>,
}

fn storage_of(host: &EngineHost, storage: Option<Arc<dyn RecordStore>>) -> Result<Arc<dyn core::RecordStore>, EngineError> {
    if let Some(storage) = storage {
        return Ok(Arc::new(Store(storage)));
    }
    let Some(dir) = &host.data_dir else { return Ok(Arc::new(eengine::MemoryRecordStore::new())) };
    #[cfg(feature = "sqlite")]
    {
        std::fs::create_dir_all(dir).map_err(|e| invalid(format!("dataDir: {e}")))?;
        let store = eengine::SqliteRecordStore::open(std::path::Path::new(dir).join("records.sqlite"))?;
        Ok(Arc::new(store))
    }
    #[cfg(not(feature = "sqlite"))]
    {
        let _ = dir;
        Err(invalid("storage: pass a RecordStore, or build eengine-ffi with the `sqlite` feature"))
    }
}

impl From<core::Failure> for EngineError {
    fn from(failure: core::Failure) -> Self {
        EngineError::Failure { code: failure.code, reason: failure.message }
    }
}

// --- Engine -------------------------------------------------------------

/// One per host process: the host config every instance runs over.
#[derive(uniffi::Object)]
pub struct Engine {
    config: core::HostConfig,
    #[cfg(feature = "hub")]
    hub: std::sync::OnceLock<emcp::hub::Hub>,
}

#[uniffi::export]
impl Engine {
    #[uniffi::constructor]
    pub fn new(host: EngineHost, identity: EngineIdentity, providers: EngineProviders) -> Result<Arc<Self>, EngineError> {
        let storage = storage_of(&host, providers.storage)?;
        let blobs: Arc<dyn core::BlobStore> = match (providers.blobs, &host.data_dir) {
            (Some(blobs), _) => Arc::new(Blobs(blobs)),
            (None, Some(dir)) => Arc::new(eengine::FsBlobStore::new(std::path::Path::new(dir).join("blobs"))),
            (None, None) => Arc::new(MemoryBlobs::default()),
        };
        let mut names = std::collections::HashSet::new();
        let mut capabilities: Vec<Arc<dyn eengine::Capability>> = Vec::new();
        for inner in providers.capabilities {
            let name = inner.name();
            if !eengine::broker::capability_name_valid(&name) {
                return Err(invalid(format!("capabilities: `{name}` is not a catalog capability")));
            }
            if !names.insert(name.clone()) {
                return Err(invalid(format!("capabilities: `{name}` twice")));
            }
            capabilities.push(Arc::new(Native { name, inner }));
        }
        let config = core::HostConfig {
            host: core::HostInfo { kind: host.kind, platform: host.platform, form_factor: host.form_factor, locale: host.locale },
            storage,
            blobs,
            identity: Arc::new(Who(identity.into())),
            vault: Arc::new(Secrets(providers.vault)),
            window: Arc::new(Windows(providers.window)),
            clock: match providers.clock {
                Some(clock) => Arc::new(ForeignClock(clock)),
                None => Arc::new(core::SystemClock),
            },
            zone: host.zone.map(|z| Arc::new(Zone(z)) as Arc<dyn core::HostZone>),
            network: match host.network {
                NetworkPolicy::Off => core::NetworkPolicy::Off,
                NetworkPolicy::UpstreamsOnly => core::NetworkPolicy::UpstreamsOnly,
                NetworkPolicy::Any => core::NetworkPolicy::Any,
            },
            sync: providers.sync.map(|s| Arc::new(Transport(s)) as Arc<dyn core::SyncTransport>),
            capabilities,
            consent: providers.consent.map(|c| Arc::new(Consent(c)) as Arc<dyn eengine::ConsentUi>),
            audit: providers.audit.map(|a| Arc::new(Audit(a)) as Arc<dyn eengine::CallAudit>),
        };
        Ok(Arc::new(Engine {
            config,
            #[cfg(feature = "hub")]
            hub: std::sync::OnceLock::new(),
        }))
    }

    /// Load a package file (`{package, signature?}`) or a bare package:
    /// verify its signature, validate it, reconcile its grants.
    pub fn load(self: Arc<Self>, file_json: String) -> Result<Arc<AppInstance>, EngineError> {
        let _span = tracing::debug_span!("eengine_ffi.load", bytes = file_json.len()).entered();
        let file = parse("file", &file_json)?;
        match eengine::AppInstance::load(self.config.clone(), &file) {
            Ok(instance) => Ok(Arc::new(AppInstance { engine: self.clone(), inner: Mutex::new(instance) })),
            Err(LoadError::Signature(issues)) => Err(EngineError::Load { kind: "signature".into(), issues_json: text(&issues) }),
            Err(LoadError::Invalid(issues)) => Err(EngineError::Load { kind: "invalid".into(), issues_json: text(&issues) }),
        }
    }
}

/// Every problem with a host config in its JSON spelling (providers.json
/// `validateCases`): `[{key, code}]`, empty = valid.
// --- The synced store ---------------------------------------------------

/// A host call's answer from the [`SyncedRecordStore`].
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum Submitted {
    /// The server took it: its answer.
    Done { answer_json: String },
    /// The server refused it (the entry is gone): the call fails. `invalid`
    /// = invalid input (BAD_REQUEST): a record form shows it inline.
    Refused { message: Option<String>, invalid: bool },
    /// Offline, behind others, a retry or a drop: the answer from the entry.
    Queued { answer_json: String },
}

/// One account's records (`eengine::synced`): the O1 outbox across its apps
/// over the host's [`OutboxLog`] and [`SyncTransport`], and the synced rows
/// the host feeds in. `submit` and `flush` block on the transport: call them
/// off the main thread.
#[derive(uniffi::Object)]
pub struct SyncedRecordStore(eengine::SyncedRecordStore);

#[uniffi::export]
impl SyncedRecordStore {
    #[uniffi::constructor]
    pub fn new(log: Arc<dyn OutboxLog>, transport: Arc<dyn SyncTransport>, listener: Option<Arc<dyn SyncedListener>>, clock: Option<Arc<dyn EngineClock>>) -> Result<Arc<Self>, EngineError> {
        let clock: Arc<dyn core::Clock> = match clock {
            Some(clock) => Arc::new(ForeignClock(clock)),
            None => Arc::new(core::SystemClock),
        };
        let listener = listener.map(|l| Arc::new(Heard(l)) as Arc<dyn eengine::SyncedListener>);
        Ok(Arc::new(SyncedRecordStore(eengine::SyncedRecordStore::new(Arc::new(Log(log)), Arc::new(Transport(transport)), clock, listener)?)))
    }

    /// Every entry, oldest first, as a JSON array.
    pub fn entries_json(&self) -> String {
        text(&self.0.entries())
    }

    /// The header glyph's label for `app` (None = nothing pending).
    pub fn pending_label(&self, app: String) -> Option<String> {
        self.0.pending_label(&app)
    }

    /// `app`'s synced rows with its entries laid over them, as a JSON array.
    pub fn visible_rows_json(&self, app: String, me: Option<String>) -> String {
        text(&self.0.visible_rows(&app, me.as_deref()))
    }

    /// A record write of an app (`records_upsert`, `records_delete`,
    /// `records_batch`) as ONE entry: client ids, persisted; alone and
    /// `online` it replays now and answers its first outcome, otherwise
    /// `Queued` with the answer from the entry (then call `flush`).
    pub fn submit_call(&self, app: String, function: String, args_json: String, online: bool) -> Result<Submitted, EngineError> {
        let Some(call) = eengine::record_call(&function) else { return Err(invalid(format!("`{function}` is not a record write"))) };
        let args = parse("args", &args_json)?;
        let entry = self.0.entry(&app, call, &args);
        let queued = entry.answer();
        Ok(match self.0.submit(entry, online)? {
            eengine::Submitted::Done(answer) => Submitted::Done { answer_json: text(&answer) },
            eengine::Submitted::Refused(pushed) => Submitted::Refused { message: pushed.message, invalid: pushed.invalid },
            eengine::Submitted::Queued => Submitted::Queued { answer_json: text(&queued) },
        })
    }

    /// Replay the pending entries until none is left or one must retry.
    pub fn flush(&self) {
        self.0.flush();
    }

    /// Feed `app`'s synced rows in (JSON array of rows): acked entries they reflect leave.
    pub fn set_rows(&self, app: String, rows_json: String) -> Result<(), EngineError> {
        let rows = serde_json::from_str(&rows_json).map_err(|e| invalid(format!("rows: {e}")))?;
        self.0.set_rows(&app, rows);
        Ok(())
    }

    /// Drop the acked entries reflected or past the grace.
    pub fn prune(&self) {
        self.0.prune();
    }
}

#[uniffi::export]
pub fn validate_host_config(config_json: String) -> Result<String, EngineError> {
    let config = parse("config", &config_json)?;
    let Value::Object(config) = config else { return Err(invalid("config: not an object")) };
    Ok(text(&core::validate_host_config(&config)))
}

// --- The runner (runner.json, E6) --------------------------------------

/// The runner's form factor: `wide` from genUI's `md` breakpoint up.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum RunnerFormFactor {
    Wide,
    Compact,
}

/// `runner.json` validated: `{config, issues}` (config null on any issue).
#[uniffi::export]
pub fn runner_config(config_json: String) -> Result<String, EngineError> {
    Ok(text(&eengine::runner::runner_config_value(&parse("config", &config_json)?)))
}

/// The package the runner's painter installs: every screen wrapped in the
/// runner's genUI shell (the instance keeps the original package).
#[uniffi::export]
pub fn runner_package(package_json: String, form_factor: RunnerFormFactor) -> Result<String, EngineError> {
    let form = match form_factor {
        RunnerFormFactor::Wide => eengine::FormFactor::Wide,
        RunnerFormFactor::Compact => eengine::FormFactor::Compact,
    };
    Ok(text(&eengine::runner_package(&parse("package", &package_json)?, form)))
}

/// The components painted in place of the app when the bundle cannot load:
/// `kind` = `missing`, `config`, `signature`, `invalid` or `busy`.
#[uniffi::export]
pub fn runner_failure(kind: String) -> Result<String, EngineError> {
    let kind: eengine::RunnerFailure = serde_json::from_value(Value::String(kind.clone())).map_err(|_| invalid(format!("runner failure `{kind}`")))?;
    Ok(text(&eengine::runner_failure(kind)))
}

/// The failed-action toast's components (surface `toast`).
#[uniffi::export]
pub fn runner_toast(message: String) -> String {
    text(&eengine::runner_toast(&message))
}

/// The facade's version (= the `eengine-v*` tag it ships under).
#[uniffi::export]
pub fn engine_version() -> String {
    env!("CARGO_PKG_VERSION").into()
}

/// The cargo features this binary was built with.
#[uniffi::export]
pub fn engine_features() -> Vec<String> {
    let mut features = Vec::new();
    if cfg!(feature = "sqlite") {
        features.push("sqlite".to_string());
    }
    if cfg!(feature = "hub") {
        features.push("hub".to_string());
    }
    features
}

// --- AppInstance --------------------------------------------------------

/// What a host function call answers.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum CallResult {
    /// The call's value (JSON).
    Value { json: String },
    /// The person decides first: paint `sheet`, then `answerConsent` with
    /// the same call and `verdictJson`.
    Ask { verdict_json: String, sheet: ConsentSheet },
    /// Refused (the failed-action toast's reason).
    Refused { code: String, message: String },
}

impl From<CoreCall> for CallResult {
    fn from(result: CoreCall) -> Self {
        match result {
            CoreCall::Value(value) => CallResult::Value { json: text(&value) },
            CoreCall::Ask(verdict, sheet) => CallResult::Ask { verdict_json: text(&verdict), sheet: (&sheet).into() },
            CoreCall::Refused(failure) => CallResult::Refused { code: failure.code, message: failure.message },
        }
    }
}

/// A stored grant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum GrantValue {
    Granted,
    Denied,
}

/// An on-device agent's way in to the app's capabilities (feature `hub`):
/// MCP Proxy's loopback hub and the run's bearer token.
#[cfg(feature = "hub")]
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct AgentAccess {
    /// `http://127.0.0.1:<port>/mcp`.
    pub url: String,
    pub token: String,
    /// The tools the agent sees (`<capability>__<tool>`).
    pub tools: Vec<String>,
}

/// One running app.
#[derive(uniffi::Object)]
pub struct AppInstance {
    #[cfg_attr(not(feature = "hub"), allow(dead_code))]
    engine: Arc<Engine>,
    inner: Mutex<eengine::AppInstance>,
}

impl AppInstance {
    fn with<T>(&self, f: impl FnOnce(&mut eengine::AppInstance) -> T) -> T {
        f(&mut self.inner.lock().unwrap_or_else(|e| e.into_inner()))
    }
}

#[uniffi::export]
impl AppInstance {
    pub fn app_id(&self) -> String {
        self.with(|i| i.app_id().to_string())
    }

    /// The validated package (JSON).
    pub fn package_json(&self) -> String {
        self.with(|i| text(i.package()))
    }

    /// The verified signer's key; None = unsigned.
    pub fn signer(&self) -> Option<String> {
        self.with(|i| i.signer().map(str::to_string))
    }

    /// The package's capability manifest, the ceiling: `[{id, why, origins?}]`.
    pub fn manifest_json(&self) -> String {
        self.with(|i| text(&i.manifest()))
    }

    /// Scope id → `granted` | `denied` (absent = undecided).
    pub fn grants_json(&self) -> String {
        self.with(|i| text(&i.grants()))
    }

    /// `{phase, screen, params}`.
    pub fn state_json(&self) -> String {
        self.with(|i| text(i.state()))
    }

    /// One lifecycle event (lifecycle.json: `{"type":"launch"}`,
    /// `foreground`, `background`, `terminate`, `online`,
    /// `{"type":"deepLink","screen","params"}`, `{"type":"notificationTap",…}`).
    /// Open, persist and flush are carried out; answers every effect (JSON
    /// array: sources and automations are the host's).
    pub fn event(&self, event_json: String) -> Result<String, EngineError> {
        let _span = tracing::debug_span!("eengine_ffi.event").entered();
        let event: LifecycleEvent = serde_json::from_str(&event_json).map_err(|e| invalid(format!("event: {e}")))?;
        Ok(self.with(|i| text(&i.event(&event))))
    }

    /// A `records:<kind>?…` source's value (JSON; the outbox laid over the
    /// store's rows); None = not a records source.
    pub fn records_value(&self, source: String) -> Option<String> {
        self.with(|i| i.records_value(&source)).map(|v| text(&v))
    }

    /// A host function call from the app's UI: `records.upsert` /
    /// `records.delete` / `records.batch`, `app.navigate`, or a capability
    /// (`<capability>.<tool>`, through the broker). `newIds` = the client
    /// ids of the creates, in order.
    pub fn call(&self, name: String, args_json: String, new_ids: Vec<String>) -> Result<CallResult, EngineError> {
        let _span = tracing::debug_span!("eengine_ffi.call", name = name.as_str()).entered();
        let args = parse("args", &args_json)?;
        Ok(self.with(|i| i.call(&name, &args, &new_ids)).into())
    }

    /// The person answered the sheet of an `ask` for this call (the answer is the gesture).
    pub fn answer_consent(&self, name: String, args_json: String, verdict_json: String, answer: ConsentAnswer) -> Result<CallResult, EngineError> {
        let args = parse("args", &args_json)?;
        let verdict: Verdict = serde_json::from_str(&verdict_json).map_err(|e| invalid(format!("verdict: {e}")))?;
        Ok(self.with(|i| i.answer_consent(&name, &args, &verdict, answer.into())).into())
    }

    /// The Permissions sheet's Switch (the host's consent UI, after the
    /// person's gesture); `value` None = revoke.
    pub fn set_grant(&self, scope: String, value: Option<GrantValue>) -> Result<(), EngineError> {
        let value = value.map(|v| match v {
            GrantValue::Granted => eengine::GrantValue::Granted,
            GrantValue::Denied => eengine::GrantValue::Denied,
        });
        self.with(|i| i.set_grant(GrantVia::ConsentUi, true, &scope, value)).map_err(|reason| EngineError::Failure { code: "FORBIDDEN".into(), reason: reason.into() })
    }

    /// The Permissions sheet: a genUI surface's components (JSON array).
    pub fn permissions_sheet_json(&self) -> String {
        self.with(|i| text(&i.permissions_sheet()))
    }

    /// The outbox entries still pending or acked (JSON array; empty without a sync provider).
    pub fn outbox_json(&self) -> String {
        self.with(|i| text(&i.outbox()))
    }

    /// Entries not yet sent (the header's `{n} changes waiting to sync`).
    pub fn pending(&self) -> u32 {
        self.with(|i| i.outbox().iter().filter(|e| e.is_pending()).count() as u32)
    }

    /// Writes queue (a sync provider) or go straight to the store.
    pub fn queues(&self) -> bool {
        self.with(|i| i.mode() == OutboxMode::Queue)
    }

    /// Flush the outbox now (FIFO; a transport failure stops it).
    pub fn flush(&self) {
        self.with(|i| i.flush());
    }
}

#[cfg(feature = "hub")]
#[uniffi::export]
impl AppInstance {
    /// Serve the app's capabilities to the on-device agent run `run` over
    /// MCP Proxy's in-process hub (loopback + bearer token); every call
    /// meets the same broker as the app's UI. A new token for a run
    /// replaces its last one.
    pub fn agent_access(&self, run: String) -> Result<AgentAccess, EngineError> {
        let session = emcp::SessionId::new(run.as_str()).map_err(|_| invalid(format!("run: `{run}` is not a session id")))?;
        let hub = match self.engine.hub.get() {
            Some(hub) => hub,
            None => {
                let info = emcp::ServerInfo { name: "eengine".into(), version: engine_version(), instructions: None };
                let hub = emcp::hub::Hub::serve(info, Arc::new(NoHubAudit)).map_err(|e| EngineError::Failure { code: "INTERNAL".into(), reason: e })?;
                let _ = self.engine.hub.set(hub);
                self.engine.hub.get().expect("set above")
            }
        };
        let broker = self.with(|i| i.broker().clone());
        eengine::hub::mount_capabilities(hub, &broker).map_err(invalid)?;
        // What the hub lists: the catalog's v1 tools of the manifest's scopes the host provides.
        let tools = eengine::hub::listed_tools(&broker).into_iter().filter_map(|(capability, t)| Some(format!("{capability}__{}", t.get("name")?.as_str()?))).collect();
        let token = hub.grant(&session, &run, eengine::hub::agent_grant(&broker));
        Ok(AgentAccess { url: hub.url().to_string(), token: token.as_str().to_string(), tools })
    }

    /// The run's token dies (feature `hub`).
    pub fn revoke_agent(&self, run: String) {
        if let (Some(hub), Ok(session)) = (self.engine.hub.get(), emcp::SessionId::new(run.as_str())) {
            hub.revoke(&session);
        }
    }
}

/// The hub's own rows: every capability call already leaves the broker's
/// row in the host's `CallAudit`.
#[cfg(feature = "hub")]
struct NoHubAudit;

#[cfg(feature = "hub")]
impl emcp::hub::AuditSink for NoHubAudit {
    fn record(&self, _row: emcp::contract::hub::AuditRow) {}
}
