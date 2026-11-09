//! VAPP-261: an `AppInstance`, one running app over a host's providers:
//! load and verify its package, drive its lifecycle (lifecycle.json) and
//! carry out the effects, resolve `records:<kind>` sources against the store
//! with genUI's one records query, dispatch `records.*`, `app.navigate` and
//! capability calls (the broker), and keep O1's outbox when the host has a
//! sync provider (outbox.json: direct writes otherwise).

use std::collections::HashMap;

use serde_json::{json, Value};

use std::sync::Arc;

use crate::broker::{CallFrom, GrantValue, Grants, ManifestEntry, Verdict};
use crate::capability::{Broker, BrokerAnswer, BrokerSetup, ConsentSheet};
use crate::consent::{grants_on_package, ConsentAnswer, GrantVia};
use crate::lifecycle::{lifecycle_step, Effect, LifecycleContext, LifecycleEvent, LifecycleState, Phase};
use crate::outbox::{outbox_flush, outbox_outcome, outbox_request, outbox_writes, OutboxCall, OutboxEntry, OutboxMode, OutboxOutcome, OutboxState, OutboxWrite};
use crate::package::{package_of, signer_of, validate, verify_package_file, Issue, Verification};
use crate::providers::{engine_record_refusal, engine_viewer, Failure, HostConfig, Presentation, RecordWrite, WriteBatch};

/// Why a package could not load.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LoadError {
    /// The file is signed and the signature does not verify.
    Signature(Vec<Issue>),
    /// The package fails genUI's validation.
    Invalid(Vec<Issue>),
}

/// What a host function call answers.
#[derive(Clone, Debug, PartialEq)]
pub enum CallResult {
    /// The call's value.
    Value(Value),
    /// The person decides first (consent.json): paint the sheet, then
    /// [`AppInstance::answer_consent`] with the same call.
    Ask(Verdict, ConsentSheet),
    /// Refused (the failed-action toast's reason).
    Refused(Failure),
}

/// Where the instance keeps its last screen (lifecycle.json `persist`).
pub fn screen_key(app: &str) -> String {
    format!("eengine/screen/{app}")
}

/// One running app.
pub struct AppInstance {
    config: HostConfig,
    pkg: Value,
    app_id: String,
    signer: Option<String>,
    broker: Arc<Broker>,
    state: LifecycleState,
    outbox: Vec<OutboxEntry>,
}

impl AppInstance {
    /// Load a package file (or a bare package): verify its signature when it
    /// carries one, validate the package, reconcile its grants.
    pub fn load(config: HostConfig, file: &Value) -> Result<Self, LoadError> {
        let _span = tracing::debug_span!("engine.load").entered();
        let verification = verify_package_file(file);
        if let Verification::Invalid { issues } = &verification {
            return Err(LoadError::Signature(issues.clone()));
        }
        // Round 9 §12: a format 5 file runs as its upgrade while the bridge is on.
        let raw = package_of(file);
        let pkg = if eui_core::catalog::ACCEPT_V09 { eui_core::upgrade09::upgrade_package_body09(raw) } else { raw.clone() };
        let issues = validate(&pkg);
        if !issues.is_empty() {
            return Err(LoadError::Invalid(issues));
        }
        let app_id = pkg.get("id").and_then(Value::as_str).unwrap_or_default().to_string();
        let signer = signer_of(&verification).map(str::to_string);
        let manifest = ManifestEntry::of_package(&pkg);
        let key = crate::consent::grants_record(&app_id, signer.as_deref().unwrap_or(""), &manifest, &Grants::new()).0;
        let grants = grants_on_package(config.vault.read(&key).as_deref(), signer.as_deref().unwrap_or(""), &manifest);
        let broker = Arc::new(Broker::new(BrokerSetup {
            app_id: app_id.clone(),
            app_name: pkg.get("name").and_then(Value::as_str).unwrap_or(&app_id).to_string(),
            signer: signer.clone().unwrap_or_default(),
            manifest,
            grants,
            capabilities: config.capabilities.clone(),
            vault: config.vault.clone(),
            blobs: config.blobs.clone(),
            screens: eui_core::host::package_screens(&pkg).iter().filter_map(|s| s.get("template").and_then(Value::as_str).map(str::to_string)).collect(),
            consent: config.consent.clone(),
            audit: config.audit.clone(),
        }));
        Ok(AppInstance { config, pkg, app_id, signer, broker, state: LifecycleState::default(), outbox: Vec::new() })
    }

    pub fn app_id(&self) -> &str {
        &self.app_id
    }

    pub fn package(&self) -> &Value {
        &self.pkg
    }

    pub fn signer(&self) -> Option<&str> {
        self.signer.as_deref()
    }

    pub fn manifest(&self) -> &[ManifestEntry] {
        self.broker.manifest()
    }

    pub fn grants(&self) -> Grants {
        self.broker.grants()
    }

    /// The instance's broker (the hub mounts its capabilities through it, `hub`).
    pub fn broker(&self) -> &Arc<Broker> {
        &self.broker
    }

    /// The person answered the sheet of an `Ask` for this call (the answer is the gesture).
    pub fn answer_consent(&mut self, name: &str, args: &Value, verdict: &Verdict, answer: ConsentAnswer) -> CallResult {
        brokered(self.broker.answer(CallFrom::Ui, None, name, args, verdict, answer))
    }

    /// The Permissions sheet's Switch: only the ConsentUi, after a gesture.
    pub fn set_grant(&mut self, via: GrantVia, gesture: bool, scope: &str, value: Option<GrantValue>) -> Result<(), &'static str> {
        self.broker.set_grant(via, gesture, scope, value)
    }

    /// The Permissions sheet (a genUI surface).
    pub fn permissions_sheet(&self) -> Vec<Value> {
        self.broker.permissions_sheet()
    }

    pub fn state(&self) -> &LifecycleState {
        &self.state
    }

    /// The outbox entries still pending or acked (queue mode).
    pub fn outbox(&self) -> &[OutboxEntry] {
        &self.outbox
    }

    pub fn mode(&self) -> OutboxMode {
        OutboxMode::of(self.config.sync.is_some())
    }

    fn me(&self) -> String {
        engine_viewer(&self.config.identity.identity()).id
    }

    /// One lifecycle event: the next state, with its effects carried out
    /// (open → the WindowHost, persist → the Vault, flush → the outbox).
    /// Answers the effects (sources and automations are the host's).
    pub fn event(&mut self, event: &LifecycleEvent) -> Vec<Effect> {
        let _span = tracing::debug_span!("engine.event", app = self.app_id.as_str()).entered();
        let stored = self.config.vault.read(&screen_key(&self.app_id)).and_then(|s| serde_json::from_str::<Value>(&s).ok());
        let last = stored.as_ref().and_then(|v| Some((v.get("screen")?.as_str()?, v.get("params")?)));
        let cx = LifecycleContext { pkg: &self.pkg, last_screen: last };
        let (next, effects) = lifecycle_step(&self.state, event, &cx);
        self.state = next;
        self.broker.set_foreground(self.state.phase == Phase::Foreground);
        for effect in &effects {
            match effect {
                Effect::Open { screen, params } => self.config.window.open(&self.app_id, screen, params, Presentation::Replace),
                Effect::Persist { screen, params } => {
                    let _ = self.config.vault.write(&screen_key(&self.app_id), &json!({"screen": screen, "params": params}).to_string());
                }
                Effect::Flush => self.flush(),
                _ => {}
            }
        }
        effects
    }

    /// A `records:<kind>?…` source's value (genUI's records query over the
    /// store's rows with the outbox laid over them); `None` = not a records source.
    pub fn records_value(&self, source: &str) -> Option<Value> {
        let _span = tracing::trace_span!("engine.records_value").entered();
        let parsed = eui_core::host::parse_source(source)?;
        let query = eui_core::host::parse_records_source(&parsed)?;
        if query.app.is_some() {
            return Some(Value::Null);
        }
        let mut rows = self.config.storage.rows(&self.app_id).ok()?;
        if !self.outbox.is_empty() {
            rows = overlay(rows, &self.outbox, Some(&self.me()));
        }
        let records = self.pkg.get("records");
        let computed = eui_core::host::kind_computed(records, query.from.as_deref());
        let refs = eui_core::host::kind_refs(records, query.from.as_deref());
        let transitions = eui_core::host::kind_transitions(query.from.as_deref().and_then(|k| records.and_then(|r| r.get(k))));
        let now = self.config.clock.now();
        let me = self.me();
        let viewer = engine_viewer(&self.config.identity.identity());
        let names = HashMap::new();
        let ctx = eui_core::host::RecordsContext {
            me: Some(&me),
            now: &now,
            today: None,
            names: &names,
            computed: computed.as_ref(),
            refs: refs.as_ref(),
            issues: None,
            transitions: transitions.as_ref(),
            role: viewer.role.as_deref(),
        };
        let row_refs: Vec<eui_core::host::RecordRowRef> = rows.iter().map(eui_core::host::RecordRow::as_ref).collect();
        eui_core::host::records_value(&row_refs, &query, ctx)
    }

    /// A host function call from the app's UI: `records.upsert` /
    /// `records.delete` / `records.batch`, `app.navigate`, or a capability
    /// (`<capability>.<tool>`, through the broker).
    pub fn call(&mut self, name: &str, args: &Value, new_ids: &[String]) -> CallResult {
        let _span = tracing::debug_span!("engine.call", name).entered();
        match name {
            "records_upsert" => self.write(OutboxCall::Upsert, args, new_ids),
            "records_delete" => self.write(OutboxCall::Delete, args, new_ids),
            "records_batch" => self.write(OutboxCall::Batch, args, new_ids),
            "app_navigate" => match eui_core::host::navigate_target(&self.pkg, args) {
                Ok(target) => {
                    self.config.window.open(&self.app_id, &target.screen, &target.params, Presentation::Replace);
                    self.state.screen = Some(target.screen.clone());
                    self.state.params = Some(target.params.clone());
                    CallResult::Value(Value::Null)
                }
                Err(e) => CallResult::Refused(Failure::new("BAD_REQUEST", e)),
            },
            _ => brokered(self.broker.call(CallFrom::Ui, None, name, args)),
        }
    }

    /// The kind policy check for a write, as the identity sees it.
    fn refusal(&self, writes: &[OutboxWrite]) -> Option<String> {
        let identity = self.config.identity.identity();
        let rows = self.config.storage.rows(&self.app_id).unwrap_or_default();
        for w in writes {
            let (kind, op, row) = match w {
                OutboxWrite::Create { kind, .. } => (Some(kind.clone()), eui_core::host::RecordOp::Create, None),
                OutboxWrite::Update { id, kind, .. } => (Some(kind.clone()), eui_core::host::RecordOp::Update, rows.iter().find(|r| r.id == *id)),
                OutboxWrite::Delete { id } => {
                    let row = rows.iter().find(|r| r.id == *id);
                    (row.map(|r| r.kind.clone()), eui_core::host::RecordOp::Delete, row)
                }
            };
            let def = kind.as_deref().and_then(|k| self.pkg.get("records").and_then(|r| r.get(k)));
            let policy = eui_core::host::record_policy(def);
            let row = row.map(|r| json!({"authorId": r.author_id, "createdBy": r.created_by}));
            if let Some(refusal) = engine_record_refusal(&identity, &policy, op, row.as_ref()) {
                return Some(refusal.to_string());
            }
        }
        None
    }

    fn write(&mut self, call: OutboxCall, args: &Value, new_ids: &[String]) -> CallResult {
        let writes = outbox_writes(call, args, new_ids);
        if let Some(refusal) = self.refusal(&writes) {
            return CallResult::Refused(Failure::new("FORBIDDEN", refusal));
        }
        let at = self.config.clock.now();
        let write_id = uuid::Uuid::new_v4().to_string();
        let entry = OutboxEntry { write_id: write_id.clone(), app_id: self.app_id.clone(), call, writes, at, state: OutboxState::Pending, acked_at: None };
        let answer = entry.answer();
        match self.mode() {
            OutboxMode::Direct => {
                let me = self.me();
                match self.config.storage.write(&self.app_id, &batch_of(&entry), Some(&me)) {
                    Ok(_) => CallResult::Value(answer),
                    Err(e) => CallResult::Refused(e),
                }
            }
            OutboxMode::Queue => {
                self.outbox.push(entry);
                if self.state.phase == Phase::Foreground {
                    self.flush();
                }
                CallResult::Value(answer)
            }
        }
    }

    /// Flush the outbox through the sync provider (FIFO, a retry stops it).
    pub fn flush(&mut self) {
        let Some(sync) = self.config.sync.clone() else { return };
        let mut results = HashMap::new();
        for entry in self.outbox.iter().filter(|e| e.is_pending()) {
            let (call, input) = outbox_request(entry);
            let result = sync.push(&self.app_id, call.as_str(), &input);
            let stop = outbox_outcome(entry, &result) == OutboxOutcome::Retry;
            results.insert(entry.write_id.clone(), result);
            if stop {
                break;
            }
        }
        if results.is_empty() {
            return;
        }
        let now = self.config.clock.now();
        self.outbox = outbox_flush(&self.outbox, &results, &now).after;
        // Acked entries keep overlaying until the store reflects them (outboxPrune).
        let rows: Vec<crate::outbox::OutboxRow> = self.config.storage.rows(&self.app_id).unwrap_or_default().into_iter().map(outbox_row).collect();
        self.outbox = crate::outbox::outbox_prune(&self.outbox, &rows, &now);
    }
}

/// VAPP-271: app UI reads a capability's answer as its `structuredContent`
/// (an action step's `results.<name>`); an `isError` result is the call's
/// refusal, its text the toast. Every host (the desktop runner, the FFI)
/// maps a broker answer through this one function.
pub fn brokered(answer: BrokerAnswer) -> CallResult {
    match answer {
        BrokerAnswer::Result(result) if result.get("isError").and_then(Value::as_bool) == Some(true) => {
            let text = result.pointer("/content/0/text").and_then(Value::as_str).unwrap_or("failed");
            CallResult::Refused(Failure::new("failed", text))
        }
        BrokerAnswer::Result(result) => CallResult::Value(result.get("structuredContent").cloned().unwrap_or(Value::Null)),
        BrokerAnswer::Ask(verdict, sheet) => CallResult::Ask(verdict, sheet),
        BrokerAnswer::Refused(failure) => CallResult::Refused(failure),
    }
}

/// An outbox entry as one store batch.
pub fn batch_of(entry: &OutboxEntry) -> WriteBatch {
    let writes = entry
        .writes
        .iter()
        .map(|w| match w {
            OutboxWrite::Create { id, kind, data } => RecordWrite::Upsert { id: id.clone(), kind: kind.clone(), data: data.clone(), create: true },
            OutboxWrite::Update { id, kind, data } => RecordWrite::Upsert { id: id.clone(), kind: kind.clone(), data: data.clone(), create: false },
            OutboxWrite::Delete { id } => RecordWrite::Delete { id: id.clone() },
        })
        .collect();
    WriteBatch { write_id: entry.write_id.clone(), writes, at: entry.at.clone() }
}

pub(crate) fn outbox_row(r: eui_core::host::RecordRow) -> crate::outbox::OutboxRow {
    crate::outbox::OutboxRow {
        id: r.id,
        kind: r.kind,
        data: r.data.as_object().cloned().unwrap_or_default(),
        created_by: r.created_by,
        author_id: r.author_id,
        created_at: r.created_at,
        updated_at: r.updated_at,
    }
}

pub(crate) fn overlay(rows: Vec<eui_core::host::RecordRow>, entries: &[OutboxEntry], me: Option<&str>) -> Vec<eui_core::host::RecordRow> {
    let rows: Vec<crate::outbox::OutboxRow> = rows.into_iter().map(outbox_row).collect();
    crate::outbox::outbox_rows(&rows, entries, me)
        .into_iter()
        .map(|r| eui_core::host::RecordRow { id: r.id, kind: r.kind, data: Value::Object(r.data), created_by: r.created_by, author_id: r.author_id, created_at: r.created_at, updated_at: r.updated_at })
        .collect()
}
