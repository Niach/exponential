//! Local store (masterplan-v3 §5.4) — rusqlite/WAL per-account SQLite with a
//! Zig-style generic column upsert. gpui-free.
//!
//! Semantics ported from the proven iOS engine (`ShapeClient.swift` 121-180 +
//! `SyncManager.applyBatch`) and the Zig generic upsert:
//!
//! * **One transaction per batch, never per row** — per-row writes were the
//!   iOS write-starvation bug. The writer lock is held only for the ~ms it
//!   takes to apply one batch; the HTTP long-poll happens entirely outside it.
//! * **The cursor is persisted in the SAME transaction as the rows it
//!   describes** — a rolled-back batch never advances the offset
//!   (at-least-once delivery + idempotent upserts).
//! * **The 409 path never deletes rows eagerly** (§5.6c): `mark_needs_refetch`
//!   only rewrites the `electric_offsets` row (persisting Electric's
//!   replacement handle when present) so stale rows stay readable; the refetch
//!   batch gets a synthetic [`ShapeMessage::MustRefetch`] prepended and
//!   `apply_batch` runs the `DELETE FROM {table}` + fresh `INSERT`s inside ONE
//!   commit — a reader never observes an empty table.
//! * **Known-column allowlist** (§5.4): incoming keys are filtered to the
//!   [`ShapeSpec::columns`] set and unknowns silently dropped — the
//!   conformance fixtures themselves carry stale `due_time`/`end_time`
//!   columns, and an unfiltered `INSERT` would wedge the shape in a permanent
//!   rollback/retry loop.
//! * **Scalars are bound as scalars** (§5.4 CRITICAL): never route
//!   `serde_json::Value` through its blanket `ToSql` impl — that JSON-encodes,
//!   storing `"title"` WITH quotes and `null` as the TEXT `null`. We match the
//!   variant and bind text / SQL `NULL`, pinning ONE canonical storage form —
//!   TEXT (§5.5); hydrate coerces to native types.

use std::fmt;
use std::path::Path;
use std::sync::Mutex;

use rusqlite::types::Value as SqlValue;
use rusqlite::{params, Connection, OpenFlags, Row, TransactionBehavior};
use serde_json::{Map, Value};

use crate::protocol::{RowKey, ShapeMessage, ShapeState};
use crate::shapes::{PkKind, ShapeSpec, SHAPES};

// The tolerant hydrate deserializers moved to `domain::hydrate` (§5.5 — the
// typed row structs live in `domain` and must not depend on `sync`);
// re-exported here so store-level callers/tests keep one import path.
pub use domain::hydrate::{
    tolerant_bool, tolerant_f64, tolerant_i64, tolerant_opt_bool, tolerant_opt_f64,
    tolerant_opt_i64, tolerant_opt_json,
};

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub enum StoreError {
    Sqlite(rusqlite::Error),
    Io(std::io::Error),
    /// A message's key shape does not match the table's PK kind (e.g. a
    /// composite key on an `id` table) — the batch rolls back.
    KeyMismatch { table: &'static str },
}

impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StoreError::Sqlite(e) => write!(f, "sqlite: {e}"),
            StoreError::Io(e) => write!(f, "io: {e}"),
            StoreError::KeyMismatch { table } => {
                write!(f, "row key does not match pk kind of table {table}")
            }
        }
    }
}

impl std::error::Error for StoreError {}

impl From<rusqlite::Error> for StoreError {
    fn from(e: rusqlite::Error) -> Self {
        StoreError::Sqlite(e)
    }
}

impl From<std::io::Error> for StoreError {
    fn from(e: std::io::Error) -> Self {
        StoreError::Io(e)
    }
}

type Result<T> = std::result::Result<T, StoreError>;

// ---------------------------------------------------------------------------
// Store
// ---------------------------------------------------------------------------

/// Cursor-state table DDL (§5.4 `electric_offsets`, extended with the iOS
/// `needs_refetch`/`is_live` markers so a quit between a 409 and its refetch
/// still resumes into the atomic replacement). `"offset"` is a SQLite
/// keyword: always quoted.
const OFFSETS_DDL: &str = r#"CREATE TABLE IF NOT EXISTS electric_offsets (
  shape          TEXT PRIMARY KEY,
  handle         TEXT NOT NULL,
  "offset"       TEXT NOT NULL,
  needs_refetch  INTEGER NOT NULL DEFAULT 0,
  is_live        INTEGER NOT NULL DEFAULT 0
)"#;

/// The per-account SQLite store. One writer connection behind a `Mutex` (WAL
/// single-writer) plus a separate read-only WAL connection for hydration
/// queries that never block on the writer (§5.4).
pub struct ShapeStore {
    writer: Mutex<Connection>,
    reader: Mutex<Connection>,
}

impl ShapeStore {
    /// Open (creating if needed) the store at `path` — e.g.
    /// `{data_dir}/accounts/{account_id}/sync.sqlite` (§5.4). Applies the
    /// §5.4 pragmas and ensures the `electric_offsets` + 14 shape tables.
    ///
    /// Do NOT declare cross-table FOREIGN KEYs (§5.4): Electric delivers rows
    /// per-shape in independent streams, so a child can arrive before its
    /// parent; referential integrity is a query-time concern.
    pub fn open(path: &Path) -> Result<ShapeStore> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let writer = Connection::open(path)?;
        // execute_batch steps through (and discards) pragma result rows.
        writer.execute_batch(
            "PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON; PRAGMA busy_timeout=5000;",
        )?;
        writer.execute_batch(OFFSETS_DDL)?;
        for spec in &SHAPES {
            writer.execute_batch(&spec.ddl())?;
            // Additive schema healing: `CREATE TABLE IF NOT EXISTS` is a no-op
            // on a pre-existing table, so a build that grows a ShapeSpec column
            // would otherwise `INSERT` into a column the on-disk table lacks —
            // `no such column` → every batch rolls back → the shape wedges in
            // a permanent retry loop (the §5.4 wedge, from the disk side).
            // ALTER in any spec column the table is missing (all TEXT; removed
            // columns are left in place — hydrate ignores unknown fields).
            heal_missing_columns(&writer, spec)?;
        }
        let reader = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        reader.execute_batch("PRAGMA busy_timeout=5000;")?;
        Ok(ShapeStore {
            writer: Mutex::new(writer),
            reader: Mutex::new(reader),
        })
    }

    // -- cursor state --------------------------------------------------------

    /// Read the persisted cursor state for a shape (read-only connection).
    pub fn shape_state(&self, shape: &str) -> Result<Option<ShapeState>> {
        let conn = self.reader.lock().expect("reader poisoned");
        let mut stmt = conn.prepare_cached(
            r#"SELECT handle, "offset", needs_refetch, is_live
               FROM electric_offsets WHERE shape = ?1"#,
        )?;
        let mut rows = stmt.query(params![shape])?;
        match rows.next()? {
            Some(row) => Ok(Some(ShapeState {
                handle: row.get(0)?,
                offset: row.get(1)?,
                needs_refetch: row.get::<_, i64>(2)? != 0,
                is_live: row.get::<_, i64>(3)? != 0,
            })),
            None => Ok(None),
        }
    }

    /// The 409 / inline-must-refetch path (§5.6c step 1): persist the
    /// [`ShapeState::refetch_marker`] — replacement handle (when Electric sent
    /// one), `offset=-1`, `needs_refetch` — WITHOUT deleting any table rows.
    /// Stale rows stay visible until the refetch batch atomically replaces
    /// them; the marker survives a quit so a relaunch still resumes into the
    /// atomic replacement (iOS `ShapeClient.swift:121-135` parity).
    pub fn mark_needs_refetch(
        &self,
        shape: &str,
        replacement_handle: Option<&str>,
    ) -> Result<()> {
        let state = ShapeState::refetch_marker(replacement_handle);
        let conn = self.writer.lock().expect("writer poisoned");
        upsert_state(&conn, shape, &state)?;
        Ok(())
    }

    // -- batch apply ---------------------------------------------------------

    /// Atomically apply one decoded poll batch (§5.4): every message plus the
    /// new cursor `state` (when `Some`) inside ONE `BEGIN IMMEDIATE … COMMIT`.
    /// If anything fails the whole batch rolls back — rows and cursor stay
    /// exactly as they were, and the next poll re-requests the same offset.
    ///
    /// [`ShapeMessage::MustRefetch`] is handled as `DELETE FROM {table}` so
    /// the §5.6c synthetic head (prepended by the client on the post-409
    /// refetch) shares one commit with the fresh inserts — no empty-table
    /// flicker for readers.
    pub fn apply_batch(
        &self,
        spec: &ShapeSpec,
        msgs: &[ShapeMessage],
        state: Option<&ShapeState>,
    ) -> Result<()> {
        let mut conn = self.writer.lock().expect("writer poisoned");
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        for msg in msgs {
            match msg {
                ShapeMessage::Insert { key, value } | ShapeMessage::Update { key, value } => {
                    upsert_row(&tx, spec, key, value)?;
                }
                ShapeMessage::Delete { key } => {
                    delete_by_key(&tx, spec, key)?;
                }
                ShapeMessage::MustRefetch => {
                    tx.execute(&format!("DELETE FROM \"{}\"", spec.name), [])?;
                }
                ShapeMessage::UpToDate => { /* no row effect */ }
            }
        }
        if let Some(state) = state {
            upsert_state(&tx, spec.name, state)?;
        }
        tx.commit()?; // ONE commit per batch — never per row
        Ok(())
    }

    // -- hydrate reads (§5.5) -------------------------------------------------

    /// Read every row of a shape as a JSON object (snake_case keys, TEXT/NULL
    /// values re-wrapped as JSON) for tolerant deserialization into `domain`
    /// structs. Uses the read-only WAL connection — never blocks the writer.
    pub fn read_all(&self, spec: &ShapeSpec) -> Result<Vec<Map<String, Value>>> {
        let conn = self.reader.lock().expect("reader poisoned");
        let mut stmt = conn.prepare_cached(&format!("SELECT * FROM \"{}\"", spec.name))?;
        let names: Vec<String> = stmt
            .column_names()
            .into_iter()
            .map(|s| s.to_string())
            .collect();
        let mut rows = stmt.query([])?;
        let mut out = Vec::new();
        while let Some(row) = rows.next()? {
            out.push(row_to_map(row, &names)?);
        }
        Ok(out)
    }

    /// Point-read one row by its primary key (§5.8 — the foreground drain
    /// re-hydrates exactly the touched rows after a batch commits). `None`
    /// means the row no longer exists (it was a delete) — the collections
    /// layer removes it from the in-memory map.
    pub fn read_by_key(
        &self,
        spec: &ShapeSpec,
        key: &RowKey,
    ) -> Result<Option<Map<String, Value>>> {
        let pk_cols = spec.pk_columns();
        let pk_vals = key_values(spec, key)?;
        let where_clause = pk_cols
            .iter()
            .enumerate()
            .map(|(i, c)| format!("\"{c}\" = ?{}", i + 1))
            .collect::<Vec<_>>()
            .join(" AND ");
        let conn = self.reader.lock().expect("reader poisoned");
        let mut stmt = conn.prepare_cached(&format!(
            "SELECT * FROM \"{}\" WHERE {where_clause}",
            spec.name
        ))?;
        let names: Vec<String> = stmt
            .column_names()
            .into_iter()
            .map(|s| s.to_string())
            .collect();
        let mut rows = stmt.query(rusqlite::params_from_iter(pk_vals))?;
        match rows.next()? {
            Some(row) => Ok(Some(row_to_map(row, &names)?)),
            None => Ok(None),
        }
    }

    /// Row count of a shape table (read-only connection) — cheap UI/test probe.
    pub fn count(&self, spec: &ShapeSpec) -> Result<i64> {
        let conn = self.reader.lock().expect("reader poisoned");
        let n = conn.query_row(
            &format!("SELECT COUNT(*) FROM \"{}\"", spec.name),
            [],
            |row| row.get(0),
        )?;
        Ok(n)
    }
}

/// Re-wrap one SQLite row as a JSON object (snake_case keys, TEXT/NULL values
/// as JSON) for the §5.5 tolerant hydrate into `domain` structs.
fn row_to_map(row: &Row<'_>, names: &[String]) -> Result<Map<String, Value>> {
    let mut obj = Map::with_capacity(names.len());
    for (i, name) in names.iter().enumerate() {
        let value = match row.get_ref(i)? {
            rusqlite::types::ValueRef::Null => Value::Null,
            rusqlite::types::ValueRef::Text(t) => {
                Value::String(String::from_utf8_lossy(t).into_owned())
            }
            rusqlite::types::ValueRef::Integer(n) => Value::from(n),
            rusqlite::types::ValueRef::Real(r) => Value::from(r),
            rusqlite::types::ValueRef::Blob(_) => Value::Null,
        };
        obj.insert(name.clone(), value);
    }
    Ok(obj)
}

// ---------------------------------------------------------------------------
// Generic apply internals (Zig-style, table-agnostic)
// ---------------------------------------------------------------------------

/// Bring an existing table up to the current [`ShapeSpec::columns`] set by
/// adding any missing columns as `TEXT` (see the §5.4 wedge note at the call
/// site). Purely additive — never drops or retypes.
fn heal_missing_columns(conn: &Connection, spec: &ShapeSpec) -> Result<()> {
    let existing: Vec<String> = {
        let mut stmt = conn.prepare(&format!("PRAGMA table_info(\"{}\")", spec.name))?;
        let names = stmt
            .query_map([], |row| row.get::<_, String>(1))?
            .collect::<std::result::Result<Vec<String>, _>>()?;
        names
    };
    for col in spec.columns {
        if !existing.iter().any(|name| name == col) {
            conn.execute_batch(&format!(
                "ALTER TABLE \"{}\" ADD COLUMN \"{col}\" TEXT",
                spec.name
            ))?;
        }
    }
    Ok(())
}

fn upsert_state(conn: &Connection, shape: &str, state: &ShapeState) -> Result<()> {
    conn.execute(
        r#"INSERT INTO electric_offsets (shape, handle, "offset", needs_refetch, is_live)
           VALUES (?1, ?2, ?3, ?4, ?5)
           ON CONFLICT(shape) DO UPDATE
             SET handle = ?2, "offset" = ?3, needs_refetch = ?4, is_live = ?5"#,
        params![
            shape,
            state.handle,
            state.offset,
            state.needs_refetch as i64,
            state.is_live as i64
        ],
    )?;
    Ok(())
}

/// The PK values a [`RowKey`] carries for this table, or a mismatch error.
fn key_values(spec: &ShapeSpec, key: &RowKey) -> Result<Vec<String>> {
    match (spec.pk, key) {
        (PkKind::Id, RowKey::Single(id)) => Ok(vec![id.clone()]),
        (PkKind::IssueLabelPair, RowKey::Pair(a, b)) => Ok(vec![a.clone(), b.clone()]),
        _ => Err(StoreError::KeyMismatch { table: spec.name }),
    }
}

/// Dynamic `INSERT … ON CONFLICT(pk) DO UPDATE SET col=excluded.col, …` over
/// exactly the allowlisted columns present in the message (§5.4). Partial
/// updates touch only the delivered columns; unknown wire columns are
/// silently dropped; PK values missing from the value object are injected
/// from the row key.
fn upsert_row(
    conn: &Connection,
    spec: &ShapeSpec,
    key: &RowKey,
    value: &Map<String, Value>,
) -> Result<()> {
    let pk_cols = spec.pk_columns();
    let pk_vals = key_values(spec, key)?;

    // Known-column allowlist filter (serde_json's BTreeMap iteration makes the
    // column order deterministic → stable prepared-statement cache hits).
    let mut cols: Vec<&str> = Vec::with_capacity(value.len());
    let mut binds: Vec<SqlValue> = Vec::with_capacity(value.len() + pk_cols.len());
    for (col, val) in value {
        if spec.columns.contains(&col.as_str()) {
            cols.push(col.as_str());
            binds.push(bind_value(val));
        }
    }
    // Inject PK values from the key when the value object lacks them (delete
    // markers aside, Electric always sends them — this covers partial rows).
    for (pk_col, pk_val) in pk_cols.iter().copied().zip(&pk_vals) {
        if !cols.contains(&pk_col) {
            cols.push(pk_col);
            binds.push(SqlValue::Text(pk_val.clone()));
        }
    }

    let col_list = cols
        .iter()
        .map(|c| format!("\"{c}\""))
        .collect::<Vec<_>>()
        .join(", ");
    let placeholders = (1..=cols.len())
        .map(|i| format!("?{i}"))
        .collect::<Vec<_>>()
        .join(", ");
    let pk_list = pk_cols
        .iter()
        .map(|c| format!("\"{c}\""))
        .collect::<Vec<_>>()
        .join(", ");
    let updates = cols
        .iter()
        .filter(|c| !pk_cols.iter().any(|pk| pk == *c))
        .map(|c| format!("\"{c}\" = excluded.\"{c}\""))
        .collect::<Vec<_>>()
        .join(", ");
    let conflict = if updates.is_empty() {
        "DO NOTHING".to_string()
    } else {
        format!("DO UPDATE SET {updates}")
    };
    let sql = format!(
        "INSERT INTO \"{}\" ({col_list}) VALUES ({placeholders}) ON CONFLICT({pk_list}) {conflict}",
        spec.name
    );
    let mut stmt = conn.prepare_cached(&sql)?;
    stmt.execute(rusqlite::params_from_iter(binds))?;
    Ok(())
}

/// `DELETE … WHERE pk = ?` (or `issue_id = ? AND label_id = ?`) — the id
/// always comes from the key, never from the (possibly absent) value (§5.2).
fn delete_by_key(conn: &Connection, spec: &ShapeSpec, key: &RowKey) -> Result<()> {
    let pk_cols = spec.pk_columns();
    let pk_vals = key_values(spec, key)?;
    let where_clause = pk_cols
        .iter()
        .enumerate()
        .map(|(i, c)| format!("\"{c}\" = ?{}", i + 1))
        .collect::<Vec<_>>()
        .join(" AND ");
    let sql = format!("DELETE FROM \"{}\" WHERE {where_clause}", spec.name);
    let mut stmt = conn.prepare_cached(&sql)?;
    stmt.execute(rusqlite::params_from_iter(
        pk_vals.into_iter().map(SqlValue::Text),
    ))?;
    Ok(())
}

/// §5.4 CRITICAL: bind the underlying scalar, never `serde_json::Value`'s
/// blanket `ToSql` (which JSON-re-encodes: `"First issue"` WITH quotes, JSON
/// `null` as the TEXT `null`). Every scalar is normalized to its TEXT form —
/// the ONE canonical storage form (§5.5); real SQL `NULL` for JSON null;
/// arrays/objects (e.g. the stale fixture `description` object) as JSON text.
fn bind_value(v: &Value) -> SqlValue {
    match v {
        Value::String(s) => SqlValue::Text(s.clone()), // the text, NO quotes
        Value::Number(n) => SqlValue::Text(n.to_string()), // keep TEXT so hydrate coerces
        Value::Bool(b) => SqlValue::Text(if *b { "t" } else { "f" }.to_string()),
        Value::Null => SqlValue::Null, // real SQL NULL
        other => SqlValue::Text(serde_json::to_string(other).expect("serializing JSON value")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn tolerant_helpers_reexported_from_domain() {
        // The definitions (and their exhaustive tests) live in
        // `domain::hydrate` (§5.5); this locks the store-level re-export path.
        #[derive(serde::Deserialize)]
        struct T {
            #[serde(deserialize_with = "tolerant_bool")]
            b: bool,
            #[serde(deserialize_with = "tolerant_i64")]
            n: i64,
            #[serde(deserialize_with = "tolerant_opt_f64")]
            f: Option<f64>,
        }
        let t: T = serde_json::from_value(json!({"b": "t", "n": "3", "f": 1.5})).unwrap();
        assert_eq!((t.b, t.n, t.f), (true, 3, Some(1.5)));
    }
}
