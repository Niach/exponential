//! VAPP-261 (feature `sqlite`): `SqliteRecordStore`, the local authority of
//! a host without a sync provider (the softcoded app, eDE Local desktops,
//! exported apps). Its tables are the app-records tables `elocal` owns
//! (#EDE-55): one row per record, one event per write (R4's history, the
//! local spelling) and the `writeId` dedupe. Absorbs #SA-10's store.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

use rusqlite::{params, Connection, OptionalExtension};
use serde_json::Value;

use eui_core::host::{RecordRow, RecordsDelta};

use crate::providers::{Failure, RecordEvent, RecordStore, RecordWrite, Subscription, WriteAck, WriteBatch};
use crate::stores::apply_batch;

/// The schema (idempotent; `elocal` runs the same statements).
pub const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS app_records (
  app_id TEXT NOT NULL,
  id TEXT NOT NULL,
  kind TEXT NOT NULL,
  data TEXT NOT NULL,
  created_by TEXT,
  author_id TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  PRIMARY KEY (app_id, id)
);
CREATE INDEX IF NOT EXISTS app_records_kind ON app_records (app_id, kind);
CREATE TABLE IF NOT EXISTS app_record_events (
  seq INTEGER PRIMARY KEY AUTOINCREMENT,
  app_id TEXT NOT NULL,
  record_id TEXT NOT NULL,
  op TEXT NOT NULL,
  actor TEXT,
  write_id TEXT NOT NULL,
  patch TEXT NOT NULL,
  before TEXT NOT NULL,
  at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS app_record_events_record ON app_record_events (app_id, record_id);
CREATE TABLE IF NOT EXISTS app_record_writes (
  app_id TEXT NOT NULL,
  write_id TEXT NOT NULL,
  ids TEXT NOT NULL,
  PRIMARY KEY (app_id, write_id)
);
";

type Listener = Arc<dyn Fn(RecordsDelta) + Send + Sync>;

/// Records in a SQLite file (or in memory).
#[derive(Clone)]
pub struct SqliteRecordStore {
    db: Arc<Mutex<Connection>>,
    listeners: Arc<Mutex<(u64, HashMap<String, Vec<(u64, Listener)>>)>>,
}

fn db_err(e: rusqlite::Error) -> Failure {
    Failure::new("STORE", e.to_string())
}

fn row_of(r: &rusqlite::Row) -> rusqlite::Result<RecordRow> {
    let data: String = r.get(2)?;
    Ok(RecordRow {
        id: r.get(0)?,
        kind: r.get(1)?,
        data: serde_json::from_str(&data).unwrap_or(Value::Null),
        created_by: r.get(3)?,
        author_id: r.get(4)?,
        created_at: r.get(5)?,
        updated_at: r.get(6)?,
    })
}

const SELECT: &str = "SELECT id, kind, data, created_by, author_id, created_at, updated_at FROM app_records";

impl SqliteRecordStore {
    /// Open (and create) the store at `path`.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, Failure> {
        Self::with(Connection::open(path).map_err(db_err)?)
    }

    /// A store in memory (tests, previews).
    pub fn in_memory() -> Result<Self, Failure> {
        Self::with(Connection::open_in_memory().map_err(db_err)?)
    }

    /// A store over a connection the host already holds (`elocal`'s).
    pub fn with(db: Connection) -> Result<Self, Failure> {
        db.execute_batch(SCHEMA).map_err(db_err)?;
        Ok(SqliteRecordStore { db: Arc::new(Mutex::new(db)), listeners: Arc::default() })
    }
}

impl RecordStore for SqliteRecordStore {
    fn rows(&self, app: &str) -> Result<Vec<RecordRow>, Failure> {
        let db = self.db.lock().unwrap();
        let mut stmt = db.prepare(&format!("{SELECT} WHERE app_id = ?1 ORDER BY created_at DESC, id")).map_err(db_err)?;
        let rows = stmt.query_map(params![app], row_of).map_err(db_err)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(db_err)
    }

    fn write(&self, app: &str, batch: &WriteBatch, by: Option<&str>) -> Result<WriteAck, Failure> {
        let delta = {
            let mut db = self.db.lock().unwrap();
            let tx = db.transaction().map_err(db_err)?;
            let known: Option<String> =
                tx.query_row("SELECT ids FROM app_record_writes WHERE app_id = ?1 AND write_id = ?2", params![app, batch.write_id], |r| r.get(0)).optional().map_err(db_err)?;
            if let Some(ids) = known {
                return Ok(WriteAck { ids: serde_json::from_str(&ids).unwrap_or_default(), replayed: true });
            }
            let mut touched: Vec<RecordRow> = Vec::new();
            for write in &batch.writes {
                let id = match write {
                    RecordWrite::Upsert { id, .. } | RecordWrite::Delete { id } => id,
                };
                if touched.iter().any(|r| r.id == *id) {
                    continue;
                }
                let row = tx.query_row(&format!("{SELECT} WHERE app_id = ?1 AND id = ?2"), params![app, id], row_of).optional().map_err(db_err)?;
                touched.extend(row);
            }
            let (ack, delta, events) = apply_batch(&mut touched, batch, by)?;
            for id in &delta.deletes {
                tx.execute("DELETE FROM app_records WHERE app_id = ?1 AND id = ?2", params![app, id]).map_err(db_err)?;
            }
            for row in &delta.upserts {
                tx.execute(
                    "INSERT INTO app_records (app_id, id, kind, data, created_by, author_id, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                     ON CONFLICT (app_id, id) DO UPDATE SET kind = excluded.kind, data = excluded.data, updated_at = excluded.updated_at",
                    params![app, row.id, row.kind, row.data.to_string(), row.created_by, row.author_id, row.created_at, row.updated_at],
                )
                .map_err(db_err)?;
            }
            for e in &events {
                tx.execute(
                    "INSERT INTO app_record_events (app_id, record_id, op, actor, write_id, patch, before, at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                    params![app, e.record_id, e.op, e.actor, e.write_id, e.patch.to_string(), e.before.to_string(), e.at],
                )
                .map_err(db_err)?;
            }
            let ids = serde_json::to_string(&ack.ids).unwrap_or_else(|_| "[]".into());
            tx.execute("INSERT INTO app_record_writes (app_id, write_id, ids) VALUES (?1, ?2, ?3)", params![app, batch.write_id, ids]).map_err(db_err)?;
            tx.commit().map_err(db_err)?;
            (ack, delta)
        };
        let (ack, delta) = delta;
        let listeners: Vec<Listener> = self.listeners.lock().unwrap().1.get(app).map(|l| l.iter().map(|(_, f)| f.clone()).collect()).unwrap_or_default();
        for listener in listeners {
            listener(delta.clone());
        }
        Ok(ack)
    }

    fn history(&self, app: &str, id: &str) -> Result<Vec<RecordEvent>, Failure> {
        let db = self.db.lock().unwrap();
        let mut stmt = db
            .prepare("SELECT record_id, op, actor, write_id, patch, before, at FROM app_record_events WHERE app_id = ?1 AND record_id = ?2 ORDER BY seq")
            .map_err(db_err)?;
        let rows = stmt
            .query_map(params![app, id], |r| {
                let patch: String = r.get(4)?;
                let before: String = r.get(5)?;
                Ok(RecordEvent {
                    record_id: r.get(0)?,
                    op: r.get(1)?,
                    actor: r.get(2)?,
                    write_id: r.get(3)?,
                    patch: serde_json::from_str(&patch).unwrap_or(Value::Null),
                    before: serde_json::from_str(&before).unwrap_or(Value::Null),
                    at: r.get(6)?,
                })
            })
            .map_err(db_err)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(db_err)
    }

    fn subscribe(&self, app: &str, on: Box<dyn Fn(RecordsDelta) + Send + Sync>) -> Subscription {
        let id = {
            let mut l = self.listeners.lock().unwrap();
            l.0 += 1;
            let id = l.0;
            l.1.entry(app.to_string()).or_default().push((id, Arc::from(on)));
            id
        };
        let listeners = self.listeners.clone();
        let app = app.to_string();
        Subscription(Some(Box::new(move || {
            if let Some(list) = listeners.lock().unwrap().1.get_mut(&app) {
                list.retain(|(i, _)| *i != id);
            }
        })))
    }
}
