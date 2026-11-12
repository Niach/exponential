//! #EXP-1380 (chats.json WINDOW, results-tab.json) — the reads a chat makes
//! of the per-account store instead of the whole `messages` table, the
//! phones' queries (Android `MessageDao.rawObserveNewestByRoot` /
//! `rawObserveResultCardsByRoot` / `rawObserveByIds`):
//!
//! * [`newest_page`]: a chat's NEWEST `limit` messages (its tree, by
//!   `root_id`) plus the rows it names (its waiting cards, however old);
//! * [`result_cards`]: the tree's result CANDIDATES (its session-authored
//!   card rows) over the WHOLE chat, never the loaded page.
//!
//! Rows come back decoded and opened (EXP-1459) like a delta's. gpui-free
//! and blocking: callers run them on the background executor (#PERF-1),
//! never on the main thread.

use std::collections::HashSet;

use domain::rows::MessageRow;

use crate::store::{ShapeStore, StoreError};

/// One [`newest_page`] read.
#[derive(Debug, Default)]
pub struct NewestPage {
    /// The newest rows, then the named ones the page did not hold (no
    /// order: the reader sorts).
    pub rows: Vec<MessageRow>,
    /// How many rows the newest read returned (`>= limit` = older ones
    /// exist unloaded).
    pub newest: usize,
}

/// The newest `limit` messages of `root_id`'s tree, plus the `named` ones
/// (by id) that are not among them.
pub fn newest_page(sqlite: &ShapeStore, root_id: &str, limit: usize, named: &[String]) -> Result<NewestPage, StoreError> {
    let newest = sqlite.read_newest_messages(root_id, limit)?;
    let count = newest.len();
    let mut rows: Vec<MessageRow> = decode_messages(newest);
    let held: HashSet<&str> = rows.iter().map(|row| row.id.as_str()).collect();
    let mut missing: Vec<&str> = named.iter().map(String::as_str).filter(|id| !held.contains(id)).collect();
    missing.sort_unstable();
    missing.dedup();
    if !missing.is_empty() {
        let extra = sqlite.read_messages_by_ids(&missing)?;
        rows.extend(decode_messages(extra));
    }
    Ok(NewestPage { rows, newest: count })
}

/// The result candidates of `root_id`'s tree: every row a session posted
/// with a card (`author_session_id` set, a card), oldest first.
pub fn result_cards(sqlite: &ShapeStore, root_id: &str) -> Result<Vec<MessageRow>, StoreError> {
    let maps = sqlite.read_result_cards(root_id)?;
    Ok(decode_messages(maps))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{RowKey, ShapeMessage};
    use crate::shapes::shape_by_name;
    use serde_json::{json, Value};

    fn insert(store: &ShapeStore, rows: Vec<Value>) {
        let spec = shape_by_name("messages").unwrap();
        let msgs: Vec<ShapeMessage> = rows
            .into_iter()
            .map(|row| ShapeMessage::Insert {
                key: RowKey::Single(row["id"].as_str().unwrap().to_string()),
                value: row.as_object().unwrap().clone(),
            })
            .collect();
        store.apply_batch(spec, &msgs, None).unwrap();
    }

    /// A fresh on-disk store (WAL wants a real file), removed on drop.
    struct TempStore(std::path::PathBuf, ShapeStore);

    impl TempStore {
        fn new() -> TempStore {
            static COUNTER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
            let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let dir = std::env::temp_dir().join(format!("exp-sync-pages-test-{}-{n}", std::process::id()));
            let store = ShapeStore::open(&dir.join("sync.sqlite")).unwrap();
            TempStore(dir, store)
        }
    }

    impl Drop for TempStore {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn at(n: usize) -> String {
        format!("2026-11-12 05:{:02}:{:02}.000000+00", n / 60, n % 60)
    }

    /// A 300-message chat beside another chat: the newest page is the last
    /// `limit` of THIS tree (children included), plus a named old card.
    #[test]
    fn the_newest_page_is_the_trees_last_rows_plus_the_named_ones() {
        let temp = TempStore::new();
        let store = &temp.1;
        let mut rows: Vec<Value> = (0..300)
            .map(|n| {
                let session = if n % 10 == 0 { "c1" } else { "r1" };
                json!({"id": format!("m{n:03}"), "session_id": session, "root_id": "r1", "author_kind": "agent", "body": format!("{n}"), "created_at": at(n)})
            })
            .collect();
        rows.push(json!({"id": "other", "session_id": "r2", "root_id": "r2", "body": "elsewhere", "created_at": at(999)}));
        insert(store, rows);

        let page = newest_page(store, "r1", 200, &["m005".into(), "m299".into(), "gone".into()]).unwrap();
        assert_eq!(page.newest, 200);
        let mut ids: Vec<&str> = page.rows.iter().map(|row| row.id.as_str()).collect();
        ids.sort_unstable();
        assert_eq!(ids.len(), 201);
        assert_eq!(ids[0], "m005");
        assert_eq!(ids[1], "m100");
        assert_eq!(*ids.last().unwrap(), "m299");
        assert!(!ids.contains(&"other"));
        // A child's rows ride along (the tree, by `root_id`).
        assert!(page.rows.iter().any(|row| row.session_id == "c1"));

        let small = newest_page(store, "r2", 200, &[]).unwrap();
        assert_eq!((small.newest, small.rows.len()), (1, 1));
    }

    /// The Results candidates cover the WHOLE chat (a card far outside the
    /// newest page too), only session-authored card rows.
    #[test]
    fn result_cards_cover_rows_outside_the_window() {
        let temp = TempStore::new();
        let store = &temp.1;
        let card = json!({"kind": "surface", "title": "Report", "surface": {"root": {"id": "root", "component": "Text"}}});
        let mut rows: Vec<Value> = (0..400)
            .map(|n| json!({"id": format!("m{n:03}"), "session_id": "r1", "root_id": "r1", "author_kind": "agent", "body": "x", "created_at": at(n)}))
            .collect();
        rows.push(json!({"id": "old-result", "session_id": "r1", "root_id": "r1", "author_session_id": "c1", "card": card, "created_at": at(0)}));
        rows.push(json!({"id": "person-card", "session_id": "r1", "root_id": "r1", "author_kind": "user", "card": card, "created_at": at(1)}));
        rows.push(json!({"id": "no-card", "session_id": "r1", "root_id": "r1", "author_session_id": "c1", "body": "report", "created_at": at(2)}));
        rows.push(json!({"id": "empty-author", "session_id": "r1", "root_id": "r1", "author_session_id": "", "card": card, "created_at": at(3)}));
        rows.push(json!({"id": "other-tree", "session_id": "r2", "root_id": "r2", "author_session_id": "c9", "card": card, "created_at": at(4)}));
        insert(store, rows);

        let page = newest_page(store, "r1", 200, &[]).unwrap();
        assert!(!page.rows.iter().any(|row| row.id == "old-result"));
        let results = result_cards(store, "r1").unwrap();
        let ids: Vec<&str> = results.iter().map(|row| row.id.as_str()).collect();
        assert_eq!(ids, ["old-result"]);
        assert!(results[0].card.as_ref().is_some_and(|card| card["kind"] == "surface"));
    }

    /// The page's index: every store has it, one opened before it existed
    /// too, and both chat reads use it (never a scan of `messages`).
    #[test]
    fn the_root_index_reaches_existing_stores_and_backs_the_reads() {
        let temp = TempStore::new();
        let path = temp.0.join("sync.sqlite");
        let index = |path: &std::path::Path| -> bool {
            let conn = rusqlite::Connection::open(path).unwrap();
            conn.query_row("SELECT COUNT(*) FROM sqlite_master WHERE type = 'index' AND name = 'messages_root_created'", [], |row| row.get::<_, i64>(0)).unwrap() == 1
        };
        assert!(index(&path));
        // An existing store from before the index.
        rusqlite::Connection::open(&path).unwrap().execute_batch(r#"DROP INDEX "messages_root_created""#).unwrap();
        assert!(!index(&path));
        let reopened = ShapeStore::open(&path).unwrap();
        assert!(index(&path));
        for sql in [
            r#"SELECT * FROM "messages" WHERE "root_id" = 'r1' ORDER BY "created_at" DESC, "id" DESC LIMIT 200"#,
            r#"SELECT * FROM "messages" WHERE "root_id" = 'r1' AND "author_session_id" IS NOT NULL AND "card" IS NOT NULL ORDER BY "created_at", "id""#,
        ] {
            let plan = reopened.explain(sql).unwrap();
            assert!(plan.contains("messages_root_created"), "{plan}");
            assert!(!plan.contains("TEMP B-TREE"), "no sort: {plan}");
        }
    }
}

/// Decode and open (EXP-1459) the rows read back, dropping the unhydratable
/// ones like a delta does. Here, not `collections::decode_rows`: that module
/// is the gpui seam, and this one is read by the gpui-free CLI build too.
fn decode_messages(maps: Vec<serde_json::Map<String, serde_json::Value>>) -> Vec<MessageRow> {
    let opener = crate::sealed::Opener::global();
    maps.into_iter()
        .filter_map(|map| match serde_json::from_value::<MessageRow>(serde_json::Value::Object(map)) {
            Ok(mut row) => {
                opener.open_message(&mut row);
                Some(row)
            }
            Err(err) => {
                log::warn!("[sync messages] dropping unhydratable row: {err}");
                None
            }
        })
        .collect()
}
