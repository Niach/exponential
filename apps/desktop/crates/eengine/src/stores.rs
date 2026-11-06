//! VAPP-261: the local providers every host without its own can take: an
//! in-memory record store (tests, previews), the local identity (a device
//! uuid, no account), a filesystem blob store and an in-memory vault.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use serde_json::{Map, Value};

use eui_core::host::{RecordRow, RecordsDelta};

use crate::providers::{BlobStore, EngineIdentity, Failure, Identity, RecordEvent, RecordStore, RecordWrite, Subscription, Vault, WriteAck, WriteBatch};

type Listener = Arc<dyn Fn(RecordsDelta) + Send + Sync>;

#[derive(Default)]
struct MemoryState {
    rows: HashMap<String, Vec<RecordRow>>,
    events: HashMap<String, Vec<RecordEvent>>,
    acks: HashMap<(String, String), WriteAck>,
    listeners: HashMap<String, Vec<(u64, Listener)>>,
    next_listener: u64,
}

/// Apply one batch to `rows` (all or nothing: on a refusal nothing changed).
/// Answers the ack, the delta and the events. A shared rule of the local stores.
pub(crate) fn apply_batch(rows: &mut Vec<RecordRow>, batch: &WriteBatch, by: Option<&str>) -> Result<(WriteAck, RecordsDelta, Vec<RecordEvent>), Failure> {
    let mut next = rows.clone();
    let mut delta = RecordsDelta::default();
    let mut events = Vec::new();
    let mut ids = Vec::new();
    for write in &batch.writes {
        match write {
            RecordWrite::Upsert { id, kind, data, create } => {
                let at = next.iter().position(|r| r.id == *id);
                match at {
                    Some(i) => {
                        let row = &mut next[i];
                        if *create {
                            return Err(Failure::new("CONFLICT", format!("{id} exists")));
                        }
                        let before = row.data.clone();
                        if !row.data.is_object() {
                            row.data = Value::Object(Map::new());
                        }
                        if let Some(o) = row.data.as_object_mut() {
                            for (k, v) in data {
                                o.insert(k.clone(), v.clone());
                            }
                        }
                        row.updated_at = batch.at.clone();
                        delta.upserts.push(row.clone());
                        events.push(RecordEvent { record_id: id.clone(), op: "update".into(), actor: by.map(str::to_string), write_id: batch.write_id.clone(), patch: Value::Object(data.clone()), before, at: batch.at.clone() });
                    }
                    None => {
                        if !*create {
                            return Err(Failure::new("NOT_FOUND", format!("no record {id}")));
                        }
                        let row = RecordRow {
                            id: id.clone(),
                            kind: kind.clone(),
                            data: Value::Object(data.clone()),
                            created_by: by.map(str::to_string),
                            author_id: by.map(str::to_string),
                            created_at: batch.at.clone(),
                            updated_at: batch.at.clone(),
                        };
                        next.push(row.clone());
                        delta.upserts.push(row);
                        events.push(RecordEvent { record_id: id.clone(), op: "create".into(), actor: by.map(str::to_string), write_id: batch.write_id.clone(), patch: Value::Object(data.clone()), before: Value::Null, at: batch.at.clone() });
                    }
                }
                ids.push(id.clone());
            }
            RecordWrite::Delete { id } => {
                let Some(i) = next.iter().position(|r| r.id == *id) else { return Err(Failure::new("NOT_FOUND", format!("no record {id}"))) };
                let gone = next.remove(i);
                delta.deletes.push(id.clone());
                events.push(RecordEvent { record_id: id.clone(), op: "delete".into(), actor: by.map(str::to_string), write_id: batch.write_id.clone(), patch: Value::Null, before: gone.data, at: batch.at.clone() });
                ids.push(id.clone());
            }
        }
    }
    *rows = next;
    Ok((WriteAck { ids, replayed: false }, delta, events))
}

/// Every app's rows in memory, with history and the `writeId` dedupe.
#[derive(Default, Clone)]
pub struct MemoryRecordStore {
    state: Arc<Mutex<MemoryState>>,
}

impl MemoryRecordStore {
    pub fn new() -> Self {
        Self::default()
    }
}

impl RecordStore for MemoryRecordStore {
    fn rows(&self, app: &str) -> Result<Vec<RecordRow>, Failure> {
        Ok(self.state.lock().unwrap().rows.get(app).cloned().unwrap_or_default())
    }

    fn write(&self, app: &str, batch: &WriteBatch, by: Option<&str>) -> Result<WriteAck, Failure> {
        let (ack, delta, listeners) = {
            let mut state = self.state.lock().unwrap();
            let key = (app.to_string(), batch.write_id.clone());
            if let Some(ack) = state.acks.get(&key) {
                return Ok(WriteAck { ids: ack.ids.clone(), replayed: true });
            }
            let rows = state.rows.entry(app.to_string()).or_default();
            let (ack, delta, events) = apply_batch(rows, batch, by)?;
            for event in events {
                state.events.entry(format!("{app}/{}", event.record_id)).or_default().push(event);
            }
            state.acks.insert(key, ack.clone());
            let listeners: Vec<Listener> = state.listeners.get(app).map(|l| l.iter().map(|(_, f)| f.clone()).collect()).unwrap_or_default();
            (ack, delta, listeners)
        };
        for listener in listeners {
            listener(delta.clone());
        }
        Ok(ack)
    }

    fn history(&self, app: &str, id: &str) -> Result<Vec<RecordEvent>, Failure> {
        Ok(self.state.lock().unwrap().events.get(&format!("{app}/{id}")).cloned().unwrap_or_default())
    }

    fn subscribe(&self, app: &str, on: Box<dyn Fn(RecordsDelta) + Send + Sync>) -> Subscription {
        let id = {
            let mut state = self.state.lock().unwrap();
            state.next_listener += 1;
            let id = state.next_listener;
            state.listeners.entry(app.to_string()).or_default().push((id, Arc::from(on)));
            id
        };
        let state = self.state.clone();
        let app = app.to_string();
        Subscription(Some(Box::new(move || {
            if let Some(list) = state.lock().unwrap().listeners.get_mut(&app) {
                list.retain(|(i, _)| *i != id);
            }
        })))
    }
}

/// The local "me": one person on one device, no account (providers.json `local`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalIdentity {
    pub device_id: String,
}

impl LocalIdentity {
    /// The device uuid stored at `path`, created (a v4 uuid) on first use.
    pub fn load_or_create(path: impl Into<PathBuf>) -> std::io::Result<Self> {
        let path = path.into();
        if let Ok(text) = std::fs::read_to_string(&path) {
            let id = text.trim();
            if uuid::Uuid::parse_str(id).is_ok() {
                return Ok(LocalIdentity { device_id: id.to_string() });
            }
        }
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let id = uuid::Uuid::new_v4().to_string();
        std::fs::write(&path, &id)?;
        Ok(LocalIdentity { device_id: id })
    }
}

impl Identity for LocalIdentity {
    fn identity(&self) -> EngineIdentity {
        EngineIdentity::Local { device_id: self.device_id.clone() }
    }
}

/// Record files on disk: `<root>/<app>/<id>` (+ `<id>.json` with name and mime).
#[derive(Clone, Debug)]
pub struct FsBlobStore {
    root: PathBuf,
}

impl FsBlobStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        FsBlobStore { root: root.into() }
    }

    fn dir(&self, app: &str) -> Result<PathBuf, Failure> {
        if app.is_empty() || app.contains(['/', '\\']) || app.starts_with('.') {
            return Err(Failure::new("BAD_REQUEST", "not an app id"));
        }
        Ok(self.root.join(app))
    }
}

fn io(e: std::io::Error) -> Failure {
    Failure::new("IO", e.to_string())
}

impl BlobStore for FsBlobStore {
    fn put(&self, app: &str, name: &str, mime: &str, bytes: &[u8]) -> Result<String, Failure> {
        let dir = self.dir(app)?;
        std::fs::create_dir_all(&dir).map_err(io)?;
        let id = uuid::Uuid::new_v4().to_string();
        std::fs::write(dir.join(&id), bytes).map_err(io)?;
        let mut meta = Map::new();
        meta.insert("name".into(), Value::String(name.into()));
        meta.insert("mime".into(), Value::String(mime.into()));
        std::fs::write(dir.join(format!("{id}.json")), Value::Object(meta).to_string()).map_err(io)?;
        Ok(id)
    }

    fn get(&self, app: &str, id: &str) -> Result<Option<Vec<u8>>, Failure> {
        if uuid::Uuid::parse_str(id).is_err() {
            return Ok(None);
        }
        match std::fs::read(self.dir(app)?.join(id)) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(io(e)),
        }
    }

    fn delete(&self, app: &str, id: &str) -> Result<(), Failure> {
        if uuid::Uuid::parse_str(id).is_err() {
            return Ok(());
        }
        let dir = self.dir(app)?;
        for path in [dir.join(id), dir.join(format!("{id}.json"))] {
            match std::fs::remove_file(path) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(io(e)),
            }
        }
        Ok(())
    }
}

/// A process-local vault (tests, previews).
#[derive(Default)]
pub struct MemoryVault {
    values: Mutex<HashMap<String, String>>,
}

impl Vault for MemoryVault {
    fn read(&self, key: &str) -> Option<String> {
        self.values.lock().unwrap().get(key).cloned()
    }

    fn write(&self, key: &str, value: &str) -> Result<(), String> {
        self.values.lock().unwrap().insert(key.to_string(), value.to_string());
        Ok(())
    }

    fn delete(&self, key: &str) -> Result<(), String> {
        self.values.lock().unwrap().remove(key);
        Ok(())
    }
}
