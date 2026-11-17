//! The SQLite adapter: the store on disk, answering exactly like
//! [`crate::MemoryStore`] (it replays the same fixture).
//!
//! The index is an FTS5 TRIGRAM table over each doc's folded text
//! ([`crate::text::folded`]): a query of three or more folded chars narrows
//! the candidates through it, a shorter one through `instr`; either way the
//! core ([`crate::collect_hits`]) does the matching, so the answers never
//! drift from the reference. Blocking: callers run it off their UI thread.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;

use rusqlite::{params, Connection, OptionalExtension};

use crate::{
    all_result, chat_result, collect_hits, docs_of_event, docs_of_message, text, AllResult, ChatResult, Doc, DocKind, Hit,
    StoredEvent, StoredMessage, ToolKey,
};

pub use rusqlite::Error;

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS events (
    session_id TEXT NOT NULL,
    device_id TEXT NOT NULL,
    seq INTEGER NOT NULL,
    root_id TEXT NOT NULL,
    kind TEXT NOT NULL,
    at INTEGER NOT NULL,
    stored_at INTEGER NOT NULL,
    event TEXT NOT NULL,
    PRIMARY KEY (session_id, device_id, seq)
) WITHOUT ROWID;
CREATE INDEX IF NOT EXISTS events_root ON events (root_id);
CREATE TABLE IF NOT EXISTS tools (
    session_id TEXT NOT NULL,
    device_id TEXT NOT NULL,
    tool_id TEXT NOT NULL,
    seq INTEGER NOT NULL,
    PRIMARY KEY (session_id, device_id, tool_id)
) WITHOUT ROWID;
CREATE TABLE IF NOT EXISTS messages (
    id TEXT PRIMARY KEY,
    root_id TEXT NOT NULL,
    session_id TEXT NOT NULL,
    author_kind TEXT NOT NULL,
    body TEXT,
    created_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS messages_root ON messages (root_id);
CREATE TABLE IF NOT EXISTS docs (
    id INTEGER PRIMARY KEY,
    root_id TEXT NOT NULL,
    session_id TEXT NOT NULL,
    device_id TEXT,
    kind TEXT NOT NULL,
    at INTEGER NOT NULL,
    seq INTEGER,
    tool_id TEXT,
    message_id TEXT,
    text TEXT NOT NULL,
    folded TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS docs_event ON docs (session_id, device_id, seq);
CREATE INDEX IF NOT EXISTS docs_message ON docs (message_id);
CREATE INDEX IF NOT EXISTS docs_root ON docs (root_id);
CREATE VIRTUAL TABLE IF NOT EXISTS docs_fts USING fts5(folded, content='docs', content_rowid='id', tokenize='trigram');
CREATE TABLE IF NOT EXISTS meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
"#;

/// A session's stored span on one device.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Span {
    pub first_seq: u64,
    pub last_seq: u64,
    pub count: u64,
}

/// The store on disk (one per account).
pub struct SqliteStore {
    conn: Mutex<Connection>,
}

impl SqliteStore {
    /// Opens (creating) the store at `path`; on unix the file is the user's
    /// only (0600): the bodies in it are opened (DECRYPTED ONLY WHERE THE
    /// PLATFORM STORE IS PROTECTED).
    pub fn open(path: &Path) -> Result<SqliteStore, Error> {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let conn = Connection::open(path)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
        }
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL; PRAGMA busy_timeout=5000;")?;
        SqliteStore::with(conn)
    }

    /// A store in memory (tests, benches).
    pub fn open_in_memory() -> Result<SqliteStore, Error> {
        SqliteStore::with(Connection::open_in_memory()?)
    }

    fn with(conn: Connection) -> Result<SqliteStore, Error> {
        conn.execute_batch(SCHEMA)?;
        Ok(SqliteStore { conn: Mutex::new(conn) })
    }

    fn conn(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.conn.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Stores the events it does not hold yet (DEDUP by session + device +
    /// seq, first write wins) and indexes them; returns how many were new.
    pub fn put_events<'e>(&self, events: impl IntoIterator<Item = &'e StoredEvent>) -> Result<usize, Error> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let mut added = 0;
        {
            let mut insert = tx.prepare_cached(
                "INSERT OR IGNORE INTO events (session_id, device_id, seq, root_id, kind, at, stored_at, event) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            )?;
            let mut tool = tx.prepare_cached(
                "INSERT INTO tools (session_id, device_id, tool_id, seq) VALUES (?1, ?2, ?3, ?4) ON CONFLICT (session_id, device_id, tool_id) DO UPDATE SET seq = min(seq, excluded.seq)",
            )?;
            for event in events {
                let json = serde_json::to_string(&event.event).unwrap_or_default();
                let inserted = insert.execute(params![
                    event.session_id,
                    event.device_id,
                    event.seq as i64,
                    event.root_id,
                    event.kind(),
                    event.at(),
                    event.stored_at,
                    json
                ])?;
                if inserted == 0 {
                    continue;
                }
                added += 1;
                if let Some(id) = event.tool_id() {
                    tool.execute(params![event.session_id, event.device_id, id, event.seq as i64])?;
                }
                if let Some(doc) = docs_of_event(event) {
                    insert_doc(&tx, &doc)?;
                }
            }
        }
        tx.commit()?;
        Ok(added)
    }

    /// Stores messages (latest wins, by id) and re-indexes them.
    pub fn put_messages<'m>(&self, messages: impl IntoIterator<Item = &'m StoredMessage>) -> Result<(), Error> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        {
            let mut current = tx.prepare_cached("SELECT body, created_at, root_id FROM messages WHERE id = ?1")?;
            let mut upsert = tx.prepare_cached(
                "INSERT OR REPLACE INTO messages (id, root_id, session_id, author_kind, body, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            )?;
            for message in messages {
                let held: Option<(Option<String>, i64, String)> =
                    current.query_row(params![message.id], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?))).optional()?;
                if held.as_ref().is_some_and(|(body, at, root)| {
                    *body == message.body && *at == message.created_at && *root == message.root_id
                }) {
                    continue;
                }
                if held.is_some() {
                    delete_docs(&tx, "message_id = ?1", params![message.id])?;
                }
                upsert.execute(params![
                    message.id,
                    message.root_id,
                    message.session_id,
                    message.author_kind,
                    message.body,
                    message.created_at
                ])?;
                if let Some(doc) = docs_of_message(message) {
                    insert_doc(&tx, &doc)?;
                }
            }
        }
        tx.commit()
    }

    /// A session's events on `device_id` in seq order (`before` = only below
    /// that seq; the newest `limit` of them when given).
    pub fn read_events(
        &self,
        session_id: &str,
        device_id: &str,
        before: Option<u64>,
        limit: Option<usize>,
    ) -> Result<Vec<StoredEvent>, Error> {
        let conn = self.conn();
        let mut stmt = conn.prepare_cached(
            "SELECT root_id, seq, stored_at, event FROM events WHERE session_id = ?1 AND device_id = ?2 AND seq < ?3 ORDER BY seq DESC LIMIT ?4",
        )?;
        let before = before.map_or(i64::MAX, |seq| seq as i64);
        let limit = limit.map_or(-1, |limit| limit as i64);
        let mut out: Vec<StoredEvent> = stmt
            .query_map(params![session_id, device_id, before, limit], |row| {
                let json: String = row.get(3)?;
                Ok(StoredEvent {
                    root_id: row.get(0)?,
                    session_id: session_id.to_string(),
                    device_id: device_id.to_string(),
                    seq: row.get::<_, i64>(1)? as u64,
                    stored_at: row.get(2)?,
                    event: serde_json::from_str(&json).unwrap_or_default(),
                })
            })?
            .collect::<Result<_, _>>()?;
        out.reverse();
        Ok(out)
    }

    /// The span of `session_id`'s stored events on `device_id` (`None` =
    /// none stored).
    pub fn span(&self, session_id: &str, device_id: &str) -> Result<Option<Span>, Error> {
        let conn = self.conn();
        let (first, last, count): (Option<i64>, Option<i64>, i64) = conn.query_row(
            "SELECT min(seq), max(seq), count(*) FROM events WHERE session_id = ?1 AND device_id = ?2",
            params![session_id, device_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )?;
        Ok(match (first, last) {
            (Some(first), Some(last)) => Some(Span { first_seq: first as u64, last_seq: last as u64, count: count as u64 }),
            _ => None,
        })
    }

    /// The candidates of the folded `query` (every doc that matches is among
    /// them), with the tool rows their outputs land on.
    fn hits(&self, query: &[char], root_id: Option<&str>) -> Result<Vec<Hit>, Error> {
        let needle: String = query.iter().collect();
        let conn = self.conn();
        let root_filter = if root_id.is_some() { " AND d.root_id = ?2" } else { "" };
        let sql = if query.len() >= 3 {
            format!(
                "SELECT d.root_id, d.session_id, d.device_id, d.kind, d.at, d.seq, d.tool_id, d.message_id, d.text FROM docs_fts JOIN docs d ON d.id = docs_fts.rowid WHERE docs_fts MATCH ?1{root_filter}"
            )
        } else {
            format!(
                "SELECT d.root_id, d.session_id, d.device_id, d.kind, d.at, d.seq, d.tool_id, d.message_id, d.text FROM docs d WHERE instr(d.folded, ?1) > 0{root_filter}"
            )
        };
        let pattern = if query.len() >= 3 { format!("\"{}\"", needle.replace('"', "\"\"")) } else { needle };
        let mut stmt = conn.prepare_cached(&sql)?;
        let map = |row: &rusqlite::Row<'_>| -> rusqlite::Result<Option<Doc>> {
            let kind: String = row.get(3)?;
            let Some(kind) = DocKind::parse(&kind) else { return Ok(None) };
            Ok(Some(Doc {
                root_id: row.get(0)?,
                session_id: row.get(1)?,
                device_id: row.get(2)?,
                kind,
                at: row.get(4)?,
                seq: row.get::<_, Option<i64>>(5)?.map(|seq| seq as u64),
                tool_id: row.get(6)?,
                message_id: row.get(7)?,
                text: row.get(8)?,
            }))
        };
        let docs: Vec<Doc> = match root_id {
            Some(root) => stmt.query_map(params![pattern, root], map)?.collect::<Result<Vec<_>, _>>()?,
            None => stmt.query_map(params![pattern], map)?.collect::<Result<Vec<_>, _>>()?,
        }
        .into_iter()
        .flatten()
        .collect();
        let mut tool_rows: HashMap<ToolKey, u64> = HashMap::new();
        {
            let mut lookup =
                conn.prepare_cached("SELECT seq FROM tools WHERE session_id = ?1 AND device_id = ?2 AND tool_id = ?3")?;
            for doc in &docs {
                let Some(tool) = &doc.tool_id else { continue };
                let key = (doc.session_id.clone(), doc.device_id.clone().unwrap_or_default(), tool.clone());
                if tool_rows.contains_key(&key) {
                    continue;
                }
                if let Some(seq) = lookup.query_row(params![key.0, key.1, key.2], |row| row.get::<_, i64>(0)).optional()? {
                    tool_rows.insert(key, seq as u64);
                }
            }
        }
        Ok(collect_hits(docs, &tool_rows, query))
    }

    /// FIND IN CHAT.
    pub fn search_chat(&self, root_id: &str, query: &str, limit: usize) -> Result<ChatResult, Error> {
        let query = text::fold_query(query);
        if query.is_empty() {
            return Ok(ChatResult::default());
        }
        Ok(chat_result(self.hits(&query, Some(root_id))?, limit))
    }

    /// GLOBAL.
    pub fn search_all(&self, query: &str, limit: usize) -> Result<AllResult, Error> {
        let query = text::fold_query(query);
        if query.is_empty() {
            return Ok(AllResult::default());
        }
        Ok(all_result(self.hits(&query, None)?, limit))
    }

    /// Applies RETENTION (`window_days` `None` = keep everything); returns
    /// how many events went.
    pub fn apply_retention(&self, now_ms: i64, window_days: Option<u32>) -> Result<usize, Error> {
        let Some(days) = window_days else { return Ok(0) };
        let cutoff = now_ms - i64::from(days) * 86_400_000;
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        // Prose always stays; any other event goes once its time is past.
        delete_docs(
            &tx,
            "seq IS NOT NULL AND EXISTS (SELECT 1 FROM events e WHERE e.session_id = docs.session_id AND e.device_id = docs.device_id \
             AND e.seq = docs.seq \
             AND e.kind NOT IN ('user_message', 'narration') AND e.at < ?1)",
            params![cutoff],
        )?;
        let removed =
            tx.execute("DELETE FROM events WHERE kind NOT IN ('user_message', 'narration') AND at < ?1", params![cutoff])?;
        tx.execute(
            "DELETE FROM tools WHERE NOT EXISTS (SELECT 1 FROM events e WHERE e.session_id = tools.session_id \
             AND e.device_id = tools.device_id AND e.seq = tools.seq)",
            [],
        )?;
        tx.commit()?;
        Ok(removed)
    }

    /// The messages stored without a body (their chat key was missing): the
    /// ones a catch-up tries to open again.
    pub fn locked_message_ids(&self) -> Result<Vec<String>, Error> {
        let conn = self.conn();
        let mut stmt = conn.prepare_cached("SELECT id FROM messages WHERE body IS NULL")?;
        let ids = stmt.query_map([], |row| row.get(0))?.collect();
        ids
    }

    /// A value of the store's own bookkeeping (catch-up watermarks).
    pub fn meta(&self, key: &str) -> Result<Option<String>, Error> {
        self.conn().query_row("SELECT value FROM meta WHERE key = ?1", params![key], |row| row.get(0)).optional()
    }

    pub fn set_meta(&self, key: &str, value: &str) -> Result<(), Error> {
        self.conn().execute("INSERT OR REPLACE INTO meta (key, value) VALUES (?1, ?2)", params![key, value]).map(|_| ())
    }
}

fn insert_doc(tx: &rusqlite::Transaction<'_>, doc: &Doc) -> Result<(), Error> {
    let folded = text::folded(&doc.text);
    tx.prepare_cached(
        "INSERT INTO docs (root_id, session_id, device_id, kind, at, seq, tool_id, message_id, text, folded) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
    )?
    .execute(params![
        doc.root_id,
        doc.session_id,
        doc.device_id,
        doc.kind.as_str(),
        doc.at,
        doc.seq.map(|seq| seq as i64),
        doc.tool_id,
        doc.message_id,
        doc.text,
        folded
    ])?;
    let id = tx.last_insert_rowid();
    tx.prepare_cached("INSERT INTO docs_fts (rowid, folded) VALUES (?1, ?2)")?.execute(params![id, folded])?;
    Ok(())
}

/// Removes the docs `filter` selects, from the index first (an external
/// content table forgets a row only when told its old value).
fn delete_docs(tx: &rusqlite::Transaction<'_>, filter: &str, args: impl rusqlite::Params + Clone) -> Result<(), Error> {
    tx.execute(
        &format!("INSERT INTO docs_fts (docs_fts, rowid, folded) SELECT 'delete', id, folded FROM docs WHERE {filter}"),
        args.clone(),
    )?;
    tx.execute(&format!("DELETE FROM docs WHERE {filter}"), args)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixture::{from, spec};
    use serde_json::json;

    fn store_of(events: &[StoredEvent], messages: &[StoredMessage]) -> SqliteStore {
        let store = SqliteStore::open_in_memory().unwrap();
        store.put_events(events).unwrap();
        store.put_messages(messages).unwrap();
        store
    }

    /// The fixture's search cases, answered by the adapter.
    #[test]
    fn search_cases() {
        let f = spec();
        let events: Vec<StoredEvent> = from(&f["search"]["state"]["events"]);
        let messages: Vec<StoredMessage> = from(&f["search"]["state"]["messages"]);
        let store = store_of(&events, &messages);
        for case in f["search"]["chatCases"].as_array().unwrap() {
            let got = store.search_chat(case["rootId"].as_str().unwrap(), case["query"].as_str().unwrap(), case["limit"].as_u64().unwrap() as usize).unwrap();
            assert_eq!(serde_json::to_value(got).unwrap(), case["expect"], "{}", case["name"]);
        }
        for case in f["search"]["allCases"].as_array().unwrap() {
            let got = store.search_all(case["query"].as_str().unwrap(), case["limit"].as_u64().unwrap() as usize).unwrap();
            assert_eq!(serde_json::to_value(got).unwrap(), case["expect"], "{}", case["name"]);
        }
    }

    #[test]
    fn ingest_cases() {
        for case in spec()["ingest"]["cases"].as_array().unwrap() {
            let store = SqliteStore::open_in_memory().unwrap();
            for batch in case["batches"].as_array().unwrap() {
                if let Some(events) = batch.get("events") {
                    store.put_events(&from::<Vec<StoredEvent>>(events)).unwrap();
                }
                if let Some(messages) = batch.get("messages") {
                    store.put_messages(&from::<Vec<StoredMessage>>(messages)).unwrap();
                }
            }
            let conn_events = |device| store.read_events("s1", device, None, None).unwrap();
            let events: Vec<StoredEvent> = conn_events("d1").into_iter().chain(conn_events("d2")).collect();
            let conn = store.conn();
            let messages: Vec<Value> = conn
                .prepare("SELECT id, body FROM messages ORDER BY id")
                .unwrap()
                .query_map([], |row| Ok(json!({ "id": row.get::<_, String>(0)?, "body": row.get::<_, Option<String>>(1)? })))
                .unwrap()
                .collect::<Result<_, _>>()
                .unwrap();
            let got = json!({
                "events": events.iter().map(|e| json!({ "sessionId": e.session_id, "deviceId": e.device_id, "seq": e.seq, "text": e.event.get("text") })).collect::<Vec<_>>(),
                "messages": messages,
            });
            assert_eq!(got, case["expect"], "{}", case["name"]);
        }
    }

    use serde_json::Value;

    #[test]
    fn retention_cases() {
        let f = spec();
        let events: Vec<StoredEvent> = from(&f["retention"]["events"]);
        for case in f["retention"]["cases"].as_array().unwrap() {
            let store = store_of(&events, &[]);
            store.apply_retention(case["now"].as_i64().unwrap(), case["windowDays"].as_u64().map(|d| d as u32)).unwrap();
            let kept: Vec<u64> = store.read_events("s1", "d1", None, None).unwrap().iter().map(|e| e.seq).collect();
            assert_eq!(json!(kept), case["expect"], "{}", case["name"]);
            // The index forgot what went: the old output is not found any more.
            let found = store.search_chat("r1", "old output", 50).unwrap().total;
            assert_eq!(found, usize::from(kept.contains(&3)), "{}", case["name"]);
        }
    }

    /// A re-synced message (opened once its key arrived) is found, the stale
    /// doc is not; short queries narrow through `instr`.
    #[test]
    fn messages_reindex_and_short_queries() {
        let store = SqliteStore::open_in_memory().unwrap();
        let sealed = StoredMessage {
            id: "m1".into(),
            root_id: "r1".into(),
            session_id: "s1".into(),
            author_kind: "agent".into(),
            body: Some("first draft".into()),
            created_at: 1,
        };
        store.put_messages([&sealed]).unwrap();
        let opened = StoredMessage { body: Some("Ok, fixed".into()), ..sealed };
        store.put_messages([&opened]).unwrap();
        assert_eq!(store.search_chat("r1", "draft", 10).unwrap().total, 0);
        assert_eq!(store.search_chat("r1", "ok", 10).unwrap().total, 1);
        assert!(store.locked_message_ids().unwrap().is_empty());
        store.put_messages([&StoredMessage { id: "m2".into(), body: None, ..opened.clone() }]).unwrap();
        assert_eq!(store.locked_message_ids().unwrap(), ["m2"]);
        assert_eq!(store.search_all("\"", 10).unwrap().total, 0);
        assert_eq!(store.search_all("fixed\"", 10).unwrap().total, 0);
    }

    #[test]
    fn spans_and_pages() {
        let events: Vec<StoredEvent> = (1..=10)
            .map(|seq| StoredEvent {
                root_id: "r".into(),
                session_id: "s".into(),
                device_id: "d".into(),
                seq,
                stored_at: 0,
                event: json!({ "kind": "narration", "text": format!("n{seq}") }),
            })
            .collect();
        let store = store_of(&events, &[]);
        assert_eq!(store.span("s", "d").unwrap(), Some(Span { first_seq: 1, last_seq: 10, count: 10 }));
        assert_eq!(store.span("s", "other").unwrap(), None);
        let page: Vec<u64> = store.read_events("s", "d", Some(8), Some(3)).unwrap().iter().map(|e| e.seq).collect();
        assert_eq!(page, [5, 6, 7]);
        assert_eq!(store.put_events(&events).unwrap(), 0);
    }

    /// On disk the store is the user's only.
    #[cfg(unix)]
    #[test]
    fn the_file_is_user_only() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("chat-store-{}-{}", std::process::id(), line!()));
        let path = dir.join("chat-store.sqlite");
        let store = SqliteStore::open(&path).unwrap();
        store.set_meta("k", "v").unwrap();
        assert_eq!(store.meta("k").unwrap().as_deref(), Some("v"));
        assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
        drop(store);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
