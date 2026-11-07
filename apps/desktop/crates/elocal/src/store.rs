//! The local store: the IDE's [`ShapeStore`] (one TEXT-column table per
//! synced shape) opened on a Local desktop's own file and written by the
//! executor instead of Electric. Rows go in and come out as TYPED JSON
//! objects in snake_case (what the server sends on the wire): the store
//! binds every value as text ("t"/"f", numbers, JSON for objects), so
//! reads coerce the columns the types table names back ([`COLUMN_TYPES`]).
//! Every write sends a [`LocalDelta`] (the table and the keys) for the host
//! to redraw, the way the sync manager's deltas do.

use std::sync::Mutex;

use serde_json::{Map, Value};
use sync::protocol::{RowKey, ShapeMessage};
use sync::shapes::{shape_by_name, ShapeSpec};
use sync::store::ShapeStore;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalDelta {
    pub table: &'static str,
    pub keys: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ColumnType {
    Integer,
    Number,
    Bool,
    Json,
}

/// The columns that are not text on the wire, by name (every table).
const COLUMN_TYPES: &[(&str, ColumnType)] = &[
    ("number", ColumnType::Integer),
    ("depth", ColumnType::Integer),
    ("estimate", ColumnType::Integer),
    ("size_bytes", ColumnType::Integer),
    ("width", ColumnType::Integer),
    ("height", ColumnType::Integer),
    ("duration_ms", ColumnType::Integer),
    ("seal_epoch", ColumnType::Integer),
    ("seal_rotate_epoch", ColumnType::Integer),
    ("head_version", ColumnType::Integer),
    ("published_version", ColumnType::Integer),
    ("schema_version", ColumnType::Integer),
    ("migration_total", ColumnType::Integer),
    ("migration_done", ColumnType::Integer),
    ("merge_attempts", ColumnType::Integer),
    ("merge_position", ColumnType::Integer),
    ("sort_order", ColumnType::Number),
    ("busy", ColumnType::Bool),
    ("needs_input", ColumnType::Bool),
    ("shared", ColumnType::Bool),
    ("plan_mode", ColumnType::Bool),
    ("computer_use", ColumnType::Bool),
    ("ultracode", ColumnType::Bool),
    ("pr_opened_automation", ColumnType::Bool),
    ("pr_merged_automation", ColumnType::Bool),
    ("end_sessions_on_merge", ColumnType::Bool),
    ("yolo_mode", ColumnType::Bool),
    ("as_file", ColumnType::Bool),
    ("enabled_by_default", ColumnType::Bool),
    ("builtin", ColumnType::Bool),
    ("unsubscribed", ColumnType::Bool),
    ("is_default", ColumnType::Bool),
    ("required", ColumnType::Bool),
    ("merge_hold", ColumnType::Bool),
    ("merge_now", ColumnType::Bool),
    ("dirty", ColumnType::Bool),
    ("triggers", ColumnType::Json),
    ("trigger_state", ColumnType::Json),
    ("data", ColumnType::Json),
    ("card", ColumnType::Json),
    ("attachment_ids", ColumnType::Json),
    ("mention_user_ids", ColumnType::Json),
    ("ref_issue_ids", ColumnType::Json),
    ("skill_ids", ColumnType::Json),
    ("mcp_server_ids", ColumnType::Json),
    ("label_ids", ColumnType::Json),
    ("package", ColumnType::Json),
    ("draft_package", ColumnType::Json),
    ("granted_tools", ColumnType::Json),
    ("payload", ColumnType::Json),
    ("caps", ColumnType::Json),
    ("agents", ColumnType::Json),
    ("unauthed_agents", ColumnType::Json),
    ("acp_agents", ColumnType::Json),
    ("launch_defaults", ColumnType::Json),
    ("agent_accounts", ColumnType::Json),
    ("agent_usage", ColumnType::Json),
    ("doctor", ColumnType::Json),
    ("active_sessions", ColumnType::Json),
    ("shared_team_ids", ColumnType::Json),
    ("issue_brief", ColumnType::Json),
    ("guide", ColumnType::Json),
    ("digest", ColumnType::Json),
    ("blocked", ColumnType::Json),
];

fn column_type(column: &str) -> Option<ColumnType> {
    COLUMN_TYPES.iter().find(|(name, _)| *name == column).map(|(_, t)| *t)
}

/// A stored TEXT value back to its wire type.
fn coerce(column: &str, value: Value) -> Value {
    let Value::String(text) = &value else { return value };
    match column_type(column) {
        Some(ColumnType::Integer) => text.parse::<i64>().map(Value::from).unwrap_or(value),
        Some(ColumnType::Number) => text.parse::<f64>().map(Value::from).unwrap_or(value),
        Some(ColumnType::Bool) => match text.as_str() {
            "t" | "true" => Value::Bool(true),
            "f" | "false" => Value::Bool(false),
            _ => value,
        },
        Some(ColumnType::Json) => serde_json::from_str(text).unwrap_or(value),
        None => value,
    }
}

/// A row as read from the store, every column in its wire type.
pub fn hydrate(mut row: Map<String, Value>) -> Map<String, Value> {
    for (column, value) in row.iter_mut() {
        let taken = std::mem::take(value);
        *value = coerce(column, taken);
    }
    row
}

pub struct LocalStore {
    shapes: ShapeStore,
    subscribers: Mutex<Vec<flume::Sender<LocalDelta>>>,
}

impl LocalStore {
    /// Opens (creating) the desktop's store at `path`.
    pub fn open(path: &std::path::Path) -> Result<Self, sync::store::StoreError> {
        let shapes = ShapeStore::open(path)?;
        Ok(Self { shapes, subscribers: Mutex::new(Vec::new()) })
    }

    /// A change stream: every write from now on (the host that redraws,
    /// the runner that claims sessions; each subscriber hears every delta).
    pub fn subscribe(&self) -> flume::Receiver<LocalDelta> {
        let (tx, rx) = flume::unbounded();
        self.subscribers.lock().expect("subscribers").push(tx);
        rx
    }

    /// [`Self::subscribe`].
    pub fn deltas(&self) -> Option<flume::Receiver<LocalDelta>> {
        Some(self.subscribe())
    }

    fn emit(&self, delta: LocalDelta) {
        let mut subscribers = self.subscribers.lock().expect("subscribers");
        subscribers.retain(|tx| tx.send(delta.clone()).is_ok());
    }

    /// The shape of a table, or `None` for a table the store does not hold.
    pub fn spec(table: &str) -> Option<&'static ShapeSpec> {
        shape_by_name(table)
    }

    fn spec_of(table: &str) -> Result<&'static ShapeSpec, String> {
        Self::spec(table).ok_or_else(|| format!("not a synced table: {table}"))
    }

    /// Every row of a table, typed.
    pub fn rows(&self, table: &str) -> Result<Vec<Map<String, Value>>, String> {
        let spec = Self::spec_of(table)?;
        Ok(self.shapes.read_all(spec).map_err(|e| e.to_string())?.into_iter().map(hydrate).collect())
    }

    /// The rows where `column` equals `value` (text compare).
    pub fn rows_where(&self, table: &str, column: &str, value: &str) -> Result<Vec<Map<String, Value>>, String> {
        Ok(self.rows(table)?.into_iter().filter(|r| r.get(column).and_then(Value::as_str) == Some(value)).collect())
    }

    /// One row by its id, typed.
    pub fn row(&self, table: &str, id: &str) -> Result<Option<Map<String, Value>>, String> {
        let spec = Self::spec_of(table)?;
        Ok(self.shapes.read_by_key(spec, &RowKey::Single(id.to_string())).map_err(|e| e.to_string())?.map(hydrate))
    }

    /// Writes (inserts or updates) one row; `row` carries its key columns.
    pub fn write(&self, table: &str, row: Map<String, Value>) -> Result<(), String> {
        self.write_many(table, vec![row])
    }

    /// Writes several rows of a table in one transaction.
    pub fn write_many(&self, table: &str, rows: Vec<Map<String, Value>>) -> Result<(), String> {
        let spec = Self::spec_of(table)?;
        let mut keys = Vec::with_capacity(rows.len());
        let msgs: Vec<ShapeMessage> = rows
            .into_iter()
            .map(|row| {
                let key = key_of(spec, &row)?;
                keys.push(key_text(&key));
                Ok(ShapeMessage::Insert { key, value: row })
            })
            .collect::<Result<_, String>>()?;
        self.shapes.apply_batch(spec, &msgs, None).map_err(|e| e.to_string())?;
        self.emit(LocalDelta { table: spec.name, keys });
        Ok(())
    }

    /// Deletes one row by id (or by `issue_id/label_id` for the pair table).
    pub fn delete(&self, table: &str, key: &str) -> Result<(), String> {
        self.delete_many(table, &[key.to_string()])
    }

    pub fn delete_many(&self, table: &str, keys: &[String]) -> Result<(), String> {
        let spec = Self::spec_of(table)?;
        let msgs: Vec<ShapeMessage> = keys.iter().map(|k| ShapeMessage::Delete { key: parse_key(spec, k) }).collect();
        self.shapes.apply_batch(spec, &msgs, None).map_err(|e| e.to_string())?;
        self.emit(LocalDelta { table: spec.name, keys: keys.to_vec() });
        Ok(())
    }
}

/// The key of a row: `id`, or `issue_id/label_id` for the pair table.
fn key_of(spec: &ShapeSpec, row: &Map<String, Value>) -> Result<RowKey, String> {
    let text = |column: &str| row.get(column).and_then(Value::as_str).map(str::to_string).ok_or_else(|| format!("{}: a row needs {column}", spec.name));
    if spec.composite_keys() {
        Ok(RowKey::Pair(text("issue_id")?, text("label_id")?))
    } else {
        Ok(RowKey::Single(text("id")?))
    }
}

fn key_text(key: &RowKey) -> String {
    match key {
        RowKey::Single(id) => id.clone(),
        RowKey::Pair(a, b) => format!("{a}/{b}"),
    }
}

fn parse_key(spec: &ShapeSpec, key: &str) -> RowKey {
    if spec.composite_keys() {
        let (a, b) = key.split_once('/').unwrap_or((key, ""));
        RowKey::Pair(a.to_string(), b.to_string())
    } else {
        RowKey::Single(key.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn obj(v: Value) -> Map<String, Value> {
        v.as_object().cloned().unwrap()
    }

    #[test]
    fn rows_round_trip_typed() {
        let dir = tempfile::tempdir().unwrap();
        let store = LocalStore::open(&dir.path().join("local.sqlite")).unwrap();
        let deltas = store.deltas().unwrap();
        store.write("issues", obj(json!({"id": "i1", "board_id": "b1", "number": 7, "identifier": "GEN-7", "title": "T", "status": "backlog", "priority": "none", "sort_order": 7, "estimate": null}))).unwrap();
        let row = store.row("issues", "i1").unwrap().unwrap();
        assert_eq!(row["number"], json!(7));
        assert_eq!(row["sort_order"], json!(7.0));
        assert_eq!(row["estimate"], Value::Null);
        assert_eq!(row["title"], json!("T"));
        store.write("sessions", obj(json!({"id": "s1", "busy": true, "shared": false, "depth": 0, "guide": {"a": 1}}))).unwrap();
        let s = store.row("sessions", "s1").unwrap().unwrap();
        assert_eq!(s["busy"], json!(true));
        assert_eq!(s["shared"], json!(false));
        assert_eq!(s["guide"], json!({"a": 1}));
        store.write("issue_labels", obj(json!({"issue_id": "i1", "label_id": "l1", "team_id": "t"}))).unwrap();
        assert_eq!(store.rows("issue_labels").unwrap().len(), 1);
        store.delete("issue_labels", "i1/l1").unwrap();
        assert_eq!(store.rows("issue_labels").unwrap().len(), 0);
        let seen: Vec<LocalDelta> = deltas.drain().collect();
        assert_eq!(seen[0], LocalDelta { table: "issues", keys: vec!["i1".into()] });
        assert_eq!(seen.len(), 4);
        assert!(store.rows("nope").is_err());
    }
}
