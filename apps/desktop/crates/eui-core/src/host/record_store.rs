//! VAPP-161: the format's NEUTRAL records (docs/package-format.md
//! "Records"). A surface reads a kind through the binding
//! `records:<kind>?query` and writes through the host functions
//! `records.upsert` / `records.delete`; the HOST provides the store. The
//! query and its value are [`super::query`] (VAPP-189: one evaluator over
//! the rows of any store, `fixtures/records-query.json`).

use std::collections::{HashMap, HashSet};

use serde::Deserialize;
use serde_json::Value;

use super::query::{records_value, RecordRow, RecordRowRef, RecordsContext};

pub use super::query::{
    parse_records_source, RecordsFilter, RecordsQuery, RECORDS_LIMIT_DEFAULT, RECORDS_LIMIT_MAX, RECORDS_ORDER_DEFAULT, RECORDS_RESERVED_PARAMS, RECORDS_SCHEME, RECORDS_SORT_DEFAULT,
};

/// The write functions (no consent card: the app's own rows); VAPP-194:
/// `records.batch {writes}` = a package action's writes as ONE unit.
pub const RECORDS_FUNCTIONS: &[&str] = &["records_upsert", "records_delete", super::actions::RECORDS_BATCH_FUNCTION];

/// VAPP-212: a change to a store's rows, or an overlay on one query (see
/// [`MemoryRecordStore::value`]). `deletes` apply first, so a row in both
/// is the upsert's.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct RecordsDelta {
    pub upserts: Vec<RecordRow>,
    pub deletes: Vec<String>,
}

/// VAPP-212: one app's rows kept on the Rust side, so a phone's query crosses
/// the FFI with the query only. A query's ties fall back to the id, so the
/// row order only shows in a sort column mixing numbers and texts (no total
/// order there). [`new`](Self::new) keeps first-seen order;
/// [`newest_first`](Self::newest_first) keeps `createdAt` descending then
/// `id`, a synced table's order, so a value depends on the rows alone and not
/// on the order their deltas arrived in.
#[derive(Debug, Clone, Default)]
pub struct MemoryRecordStore {
    rows: Vec<RecordRow>,
    at: HashMap<String, usize>,
    newest_first: bool,
}

impl MemoryRecordStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn newest_first() -> Self {
        Self { newest_first: true, ..Self::default() }
    }

    pub fn len(&self) -> usize {
        self.rows.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    /// The rows in the store's order.
    pub fn rows(&self) -> &[RecordRow] {
        &self.rows
    }

    /// Apply a sync delta: an upsert of a known id replaces the row in place,
    /// a new id joins the end (or its sorted place).
    pub fn apply(&mut self, delta: RecordsDelta) {
        if !delta.deletes.is_empty() {
            let gone: HashSet<&str> = delta.deletes.iter().map(String::as_str).collect();
            let before = self.rows.len();
            self.rows.retain(|row| !gone.contains(row.id.as_str()));
            if self.rows.len() != before {
                self.reindex();
            }
        }
        let mut moved = false;
        for row in delta.upserts {
            let known = self.at.get(&row.id).copied();
            match known {
                Some(i) => {
                    moved |= self.rows[i].created_at != row.created_at;
                    self.rows[i] = row;
                }
                None => {
                    self.at.insert(row.id.clone(), self.rows.len());
                    self.rows.push(row);
                    moved = true;
                }
            }
        }
        if moved && self.newest_first {
            self.rows.sort_by(|a, b| b.created_at.cmp(&a.created_at).then_with(|| a.id.cmp(&b.id)));
            self.reindex();
        }
    }

    fn reindex(&mut self) {
        self.at = self.rows.iter().enumerate().map(|(i, row)| (row.id.clone(), i)).collect();
    }

    /// The records value over the rows. `overlay` = the host's unsynced writes
    /// for this one query, the store untouched: its deletes hide rows, an
    /// upsert of a known id replaces the row in place, a new id joins the end
    /// in the overlay's order (`app-records.json` `outbox`).
    pub fn value(&self, query: &RecordsQuery, ctx: RecordsContext, overlay: Option<&RecordsDelta>) -> Option<Value> {
        let mut refs: Vec<RecordRowRef> = self.rows.iter().map(RecordRow::as_ref).collect();
        if let Some(overlay) = overlay.filter(|o| !o.upserts.is_empty() || !o.deletes.is_empty()) {
            let gone: HashSet<&str> = overlay.deletes.iter().map(String::as_str).collect();
            refs.retain(|row| !gone.contains(row.id));
            let mut at: HashMap<&str, usize> = refs.iter().enumerate().map(|(i, row)| (row.id, i)).collect();
            for row in &overlay.upserts {
                let known = at.get(row.id.as_str()).copied();
                match known {
                    Some(i) => refs[i] = row.as_ref(),
                    None => {
                        at.insert(row.id.as_str(), refs.len());
                        refs.push(row.as_ref());
                    }
                }
            }
        }
        records_value(&refs, query, ctx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(id: &str, created_at: &str) -> RecordRow {
        RecordRow { id: id.into(), kind: "note".into(), data: Value::Null, created_by: None, author_id: None, created_at: created_at.into(), updated_at: created_at.into() }
    }

    fn upserts(rows: &[RecordRow]) -> RecordsDelta {
        RecordsDelta { upserts: rows.to_vec(), deletes: Vec::new() }
    }

    fn ids(store: &MemoryRecordStore) -> Vec<&str> {
        store.rows().iter().map(|r| r.id.as_str()).collect()
    }

    #[test]
    fn first_seen_order_survives_updates_and_deletes() {
        let mut store = MemoryRecordStore::new();
        store.apply(upserts(&[row("b", "1"), row("a", "2"), row("c", "3")]));
        assert_eq!(ids(&store), ["b", "a", "c"]);
        store.apply(upserts(&[row("a", "9")]));
        assert_eq!(ids(&store), ["b", "a", "c"]);
        store.apply(RecordsDelta { upserts: vec![row("d", "4"), row("a", "8")], deletes: vec!["b".into()] });
        assert_eq!(ids(&store), ["a", "c", "d"]);
        assert_eq!(store.rows()[0].created_at, "8", "the index follows a delete");
    }

    #[test]
    fn newest_first_is_the_table_order_whatever_the_delta_order() {
        let mut store = MemoryRecordStore::newest_first();
        store.apply(upserts(&[row("b", "1"), row("a", "1"), row("c", "3")]));
        assert_eq!(ids(&store), ["c", "a", "b"]);
        store.apply(upserts(&[row("d", "2")]));
        assert_eq!(ids(&store), ["c", "d", "a", "b"]);
        store.apply(upserts(&[row("b", "4")]));
        assert_eq!(ids(&store), ["b", "c", "d", "a"]);
        store.apply(RecordsDelta { upserts: vec![row("a", "1")], deletes: vec!["c".into()] });
        assert_eq!(ids(&store), ["b", "d", "a"]);
        assert_eq!(store.len(), 3);
    }
}
