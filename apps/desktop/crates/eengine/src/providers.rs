//! VAPP-261: what a host hands the Engine. The provider traits, the
//! [`HostConfig`] (no telemetry and no update field: none exists), its
//! validation, the viewer each identity kind yields and the network policy.
//! Locked by `packages/eengine/fixtures/providers.json`.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use eui_core::host::{RecordRow, RecordsDelta};

use crate::broker::capability_name_valid;
use crate::contract::{strings, PROVIDERS};

/// A refusal or a failure a provider answers (eCore's shape: a code + a message).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Failure {
    pub code: String,
    pub message: String,
}

impl Failure {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Failure { code: code.into(), message: message.into() }
    }
}

impl std::fmt::Display for Failure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for Failure {}

/// One write of a batch (`app-records.json` `outbox` writes, the store's spelling).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "lowercase")]
pub enum RecordWrite {
    /// A create (the client id) or an update: the data merges shallow.
    Upsert {
        id: String,
        kind: String,
        #[serde(default)]
        data: Map<String, Value>,
        #[serde(default)]
        create: bool,
    },
    Delete { id: String },
}

/// One call's writes, all or nothing, with its idempotency key.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WriteBatch {
    pub write_id: String,
    pub writes: Vec<RecordWrite>,
    /// ISO time of the write (createdAt / updatedAt of what it touches).
    pub at: String,
}

/// What a store answers a batch: the ids in write order; `replayed` = the
/// `writeId` was already applied and nothing was written again.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WriteAck {
    pub ids: Vec<String>,
    pub replayed: bool,
}

/// One history event of a record (R4's `app_record_events`, the local spelling).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordEvent {
    pub record_id: String,
    pub op: String,
    pub actor: Option<String>,
    pub write_id: String,
    pub patch: Value,
    pub before: Value,
    pub at: String,
}

/// A live subscription; dropping it unsubscribes.
pub struct Subscription(pub Option<Box<dyn FnOnce() + Send>>);

impl Drop for Subscription {
    fn drop(&mut self) {
        if let Some(stop) = self.0.take() {
            stop();
        }
    }
}

/// Where an app's records live (generalises VAPP-212's handle). One store
/// serves every app of a host, keyed by `app`.
pub trait RecordStore: Send + Sync {
    /// Every row of `app` (the Engine runs genUI's one records query over them).
    fn rows(&self, app: &str) -> Result<Vec<RecordRow>, Failure>;
    /// Apply one batch, all or nothing; a known `write_id` answers its first ack.
    fn write(&self, app: &str, batch: &WriteBatch, by: Option<&str>) -> Result<WriteAck, Failure>;
    /// A record's events, oldest first.
    fn history(&self, app: &str, id: &str) -> Result<Vec<RecordEvent>, Failure>;
    /// Called with every change to `app`'s rows.
    fn subscribe(&self, app: &str, on: Box<dyn Fn(RecordsDelta) + Send + Sync>) -> Subscription;
}

/// Record files (format `attachment`), the photobook's photos.
pub trait BlobStore: Send + Sync {
    /// Store bytes; answers the file id.
    fn put(&self, app: &str, name: &str, mime: &str, bytes: &[u8]) -> Result<String, Failure>;
    fn get(&self, app: &str, id: &str) -> Result<Option<Vec<u8>>, Failure>;
    fn delete(&self, app: &str, id: &str) -> Result<(), Failure>;
}

/// An optional transport to an authority (Exponential: Electric in, tRPC
/// out; Spezl: P2P). Absent = the store is the authority, no outbox.
pub trait SyncTransport: Send + Sync {
    /// Replay one outbox entry (`app-records.json` `outbox` `outboxRequest`).
    fn push(&self, app: &str, call: &str, input: &Value) -> crate::outbox::OutboxResult;
}

/// Who is looking (providers.json `identityKinds`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum EngineIdentity {
    #[serde(rename_all = "camelCase")]
    Member { user_id: String, role: Option<String>, access: Option<String> },
    #[serde(rename_all = "camelCase")]
    Local { device_id: String },
    Peer { key: String },
}

pub trait Identity: Send + Sync {
    fn identity(&self) -> EngineIdentity;
}

/// The viewer genUI's record policy judges (`recordRefusal`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EngineViewer {
    pub id: String,
    pub role: Option<String>,
    pub access: String,
}

/// The viewer of an identity: `local` = the one owner of everything ("me").
pub fn engine_viewer(identity: &EngineIdentity) -> EngineViewer {
    match identity {
        EngineIdentity::Local { device_id } => EngineViewer { id: device_id.clone(), role: Some("owner".into()), access: "write".into() },
        EngineIdentity::Peer { key } => EngineViewer { id: key.clone(), role: Some("member".into()), access: "write".into() },
        EngineIdentity::Member { user_id, role, access } => {
            EngineViewer { id: user_id.clone(), role: role.clone(), access: access.clone().unwrap_or_else(|| "write".into()) }
        }
    }
}

/// Why the viewer of `identity` may not `op` a record of a kind with `policy` (`None` = allowed).
pub fn engine_record_refusal(identity: &EngineIdentity, policy: &eui_core::host::RecordPolicy, op: eui_core::host::RecordOp, row: Option<&Value>) -> Option<&'static str> {
    let viewer = engine_viewer(identity);
    let viewer = eui_core::host::RecordViewer { id: Some(&viewer.id), role: viewer.role.as_deref(), access: Some(&viewer.access) };
    eui_core::host::record_refusal(policy, op, &viewer, row)
}

/// Where secrets live (= MCP Proxy's `Vault`: Keychain, Keystore, libsecret, a sealed column).
pub trait Vault: Send + Sync {
    fn read(&self, key: &str) -> Option<String>;
    fn write(&self, key: &str, value: &str) -> Result<(), String>;
    fn delete(&self, key: &str) -> Result<(), String>;
}

/// How a host presents a screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Presentation {
    /// Replace the instance's current screen (a navigation tab, the window's content).
    Replace,
    /// Push it (a detail screen: a nav stack on phones, the same window on desktop).
    Push,
    /// A sheet over the current screen (the consent sheet).
    Sheet,
}

/// Maps screens to windows (desktop) or nav stacks and sheets (phones).
pub trait WindowHost: Send + Sync {
    fn open(&self, app: &str, screen: &str, params: &Map<String, Value>, how: Presentation);
}

/// The clock (tests pin it).
pub trait Clock: Send + Sync {
    /// ISO 8601 UTC with milliseconds.
    fn now(&self) -> String;
}

/// The system clock.
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> String {
        chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string()
    }
}

/// The IANA zone today/now/month are read in.
pub trait HostZone: Send + Sync {
    fn zone(&self) -> String;
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum NetworkPolicy {
    Off,
    #[default]
    UpstreamsOnly,
    Any,
}

/// What the instance reaches over the network.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum NetworkTarget {
    Upstream,
    Sync,
    Http,
}

/// May the instance reach `target` under `policy`? `declared` = the package
/// declares the upstream. `Err` = the refusal's reason.
pub fn network_allows(policy: NetworkPolicy, target: NetworkTarget, declared: bool) -> Result<(), &'static str> {
    match (policy, target) {
        (NetworkPolicy::Off, _) => Err("offline"),
        (_, NetworkTarget::Upstream) if !declared => Err("notDeclared"),
        (NetworkPolicy::UpstreamsOnly, NetworkTarget::Http) => Err("network"),
        _ => Ok(()),
    }
}

/// The host: its kind and platform, for the instance and the painters.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HostInfo {
    /// providers.json `hosts`: exponential, softcoded, ede, spezl, exported.
    pub kind: String,
    pub platform: String,
    pub form_factor: String,
    pub locale: String,
}

/// Everything a host hands the Engine. There is no telemetry field and no
/// update field: exported runners compile diagnostics out, and a new package
/// version is a rebundle.
#[derive(Clone)]
pub struct HostConfig {
    pub host: HostInfo,
    pub storage: Arc<dyn RecordStore>,
    pub blobs: Arc<dyn BlobStore>,
    pub identity: Arc<dyn Identity>,
    pub vault: Arc<dyn Vault>,
    pub window: Arc<dyn WindowHost>,
    pub clock: Arc<dyn Clock>,
    pub zone: Option<Arc<dyn HostZone>>,
    pub network: NetworkPolicy,
    pub sync: Option<Arc<dyn SyncTransport>>,
    /// The capability names this host mounts (E2 mounts their providers).
    pub capabilities: Vec<String>,
}

/// A host config problem (`validateCases`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostConfigIssue {
    pub key: String,
    pub code: String,
}

/// The provider keys in providers.json's order.
pub fn provider_keys() -> Vec<String> {
    PROVIDERS["providers"].as_object().map(|o| o.keys().cloned().collect()).unwrap_or_default()
}

/// Every problem with a host config in its JSON spelling (what a runner's
/// `bundle.toml` and the FFI hand in): providers in order, then forbidden
/// and unknown keys sorted.
pub fn validate_host_config(config: &Map<String, Value>) -> Vec<HostConfigIssue> {
    let mut issues = Vec::new();
    let push = |issues: &mut Vec<HostConfigIssue>, key: &str, code: &str| issues.push(HostConfigIssue { key: key.into(), code: code.into() });
    let keys = provider_keys();
    let kinds = strings(&PROVIDERS, "/identityKinds");
    let policies = strings(&PROVIDERS, "/networkPolicies");
    let agents = strings(&PROVIDERS, "/agentProviders");
    for key in &keys {
        let required = PROVIDERS["providers"][key]["required"].as_bool().unwrap_or(false);
        let Some(value) = config.get(key) else {
            if required {
                push(&mut issues, key, "missing");
            }
            continue;
        };
        match key.as_str() {
            "identity" if !value.get("kind").and_then(Value::as_str).is_some_and(|k| kinds.iter().any(|x| x == k)) => push(&mut issues, key, "invalid"),
            "network" if !value.as_str().is_some_and(|p| policies.iter().any(|x| x == p)) => push(&mut issues, key, "invalid"),
            "agents" => {
                let ids: Option<Vec<Option<&str>>> = value.as_array().map(|a| a.iter().map(|x| x.get("id").and_then(Value::as_str)).collect());
                match ids {
                    Some(ids) if ids.iter().all(|id| id.is_some_and(|id| agents.iter().any(|a| a == id))) => {
                        let mut seen = std::collections::HashSet::new();
                        if !ids.iter().all(|id| seen.insert(*id)) {
                            push(&mut issues, key, "duplicateAgent");
                        }
                    }
                    _ => push(&mut issues, key, "invalid"),
                }
            }
            "capabilities" => {
                let names: Option<Vec<Option<&str>>> = value.as_array().map(|a| a.iter().map(|x| x.get("name").and_then(Value::as_str)).collect());
                match names {
                    Some(names) if names.iter().all(|n| n.is_some_and(capability_name_valid)) => {
                        let mut seen = std::collections::HashSet::new();
                        if !names.iter().all(|n| seen.insert(*n)) {
                            push(&mut issues, key, "duplicateCapability");
                        }
                    }
                    _ => push(&mut issues, key, "invalidCapability"),
                }
            }
            _ => {}
        }
    }
    let forbidden = strings(&PROVIDERS, "/forbidden");
    let mut extra: Vec<&String> = config.keys().filter(|k| !keys.contains(k)).collect();
    extra.sort();
    for key in extra {
        push(&mut issues, key, if forbidden.contains(key) { "forbidden" } else { "unknown" });
    }
    issues
}
