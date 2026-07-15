//! Store semantics tests (masterplan-v3 §5.4 / §5.6c): batch atomicity, the
//! 409 no-flicker dance, the known-column allowlist (apply-level fixture
//! tolerance), scalar binding, and hydrate coercion.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};

use serde::Deserialize;
use serde_json::{json, Map, Value};
use sync::protocol::{parse_messages, RowKey, ShapeMessage, ShapeState};
use sync::shapes::{shape_by_name, ShapeSpec};
use sync::store::{tolerant_i64, tolerant_opt_f64, ShapeStore};

// ---------------------------------------------------------------------------
// Harness
// ---------------------------------------------------------------------------

/// A unique on-disk SQLite path per test (WAL needs a real file; in-memory
/// DBs aren't shared across the writer/reader connections). Cleaned on drop.
struct TempDb {
    path: PathBuf,
}

impl TempDb {
    fn new() -> TempDb {
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "exp-sync-store-test-{}-{n}/sync.sqlite",
            std::process::id()
        ));
        TempDb { path }
    }

    fn open(&self) -> ShapeStore {
        ShapeStore::open(&self.path).expect("store opens")
    }
}

impl Drop for TempDb {
    fn drop(&mut self) {
        if let Some(dir) = self.path.parent() {
            let _ = std::fs::remove_dir_all(dir);
        }
    }
}

fn issues() -> &'static ShapeSpec {
    shape_by_name("issues").unwrap()
}

fn insert(id: &str, value: Value) -> ShapeMessage {
    ShapeMessage::Insert {
        key: RowKey::Single(id.into()),
        value: value.as_object().cloned().expect("object"),
    }
}

fn state(handle: &str, offset: &str, is_live: bool) -> ShapeState {
    ShapeState {
        handle: handle.into(),
        offset: offset.into(),
        needs_refetch: false,
        is_live,
    }
}

fn row_by_id<'a>(rows: &'a [Map<String, Value>], id: &str) -> &'a Map<String, Value> {
    rows.iter()
        .find(|r| r.get("id").and_then(Value::as_str) == Some(id))
        .unwrap_or_else(|| panic!("row {id} missing"))
}

/// Load a single-message fixture (camel-case.json / snake-case.json) and
/// decode it through the real parser.
fn fixture_message(name: &str) -> ShapeMessage {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../../packages/electric-protocol/fixtures")
        .join(name);
    let fixture: Value = serde_json::from_slice(&std::fs::read(&path).expect("fixture")).unwrap();
    let body = serde_json::to_vec(&Value::Array(vec![fixture])).unwrap();
    let mut msgs = parse_messages(&body, false);
    assert_eq!(msgs.len(), 1);
    msgs.remove(0)
}

// ---------------------------------------------------------------------------
// §5.4 apply-level fixture tolerance (known-column allowlist)
// ---------------------------------------------------------------------------

/// §5.4: the conformance fixtures carry columns that do NOT exist in the
/// desktop `issues` table (`due_time`, `end_time`) plus a stale nested
/// `description` object. The apply must tolerate-and-drop the unknowns and
/// land the row with NO error — an unfiltered `INSERT … due_time …` would
/// roll back every batch and wedge the shape forever.
#[test]
fn apply_tolerates_and_drops_stale_fixture_columns() {
    let db = TempDb::new();
    let store = db.open();

    for name in ["snake-case.json", "camel-case.json"] {
        let msg = fixture_message(name);
        store
            .apply_batch(issues(), &[msg], Some(&state("h-1", "0_0", true)))
            .unwrap_or_else(|e| panic!("{name} must apply cleanly: {e}"));
    }

    let rows = store.read_all(issues()).unwrap();
    assert_eq!(rows.len(), 1, "both casings upsert the SAME row");
    let row = row_by_id(&rows, "01J9K0A0X3CB4E5F6G7H8J9K0L");
    assert_eq!(row.get("title").and_then(Value::as_str), Some("First issue"));
    assert_eq!(
        row.get("project_id").and_then(Value::as_str),
        Some("01J9K0A0X3CB4E5F6G7H8J9K0M")
    );
    assert_eq!(row.get("due_date").and_then(Value::as_str), Some("2026-05-20"));
    // Dropped, not stored — the columns don't even exist locally.
    assert!(!row.contains_key("due_time"));
    assert!(!row.contains_key("end_time"));
    // The stale nested description object IS a known column → JSON text
    // (bind_value's Array/Object branch), never a crash.
    assert_eq!(
        row.get("description").and_then(Value::as_str),
        Some(r#"{"text":"Body content"}"#)
    );
}

/// §5.4 forward-compat, disk side: a build that grows a ShapeSpec column must
/// heal a pre-existing (older) table at open — `CREATE TABLE IF NOT EXISTS`
/// alone would leave the on-disk table narrow and every upsert carrying the
/// new column would `no such column` → roll back → wedge the shape forever.
#[test]
fn open_heals_missing_columns_from_an_older_schema() {
    let db = TempDb::new();
    if let Some(dir) = db.path.parent() {
        std::fs::create_dir_all(dir).unwrap();
    }
    // Simulate an older build's narrower `issues` table (+ existing data).
    {
        let conn = rusqlite::Connection::open(&db.path).unwrap();
        conn.execute_batch(
            r#"CREATE TABLE "issues" ("id" TEXT NOT NULL, "title" TEXT, PRIMARY KEY ("id"));
               INSERT INTO "issues" ("id", "title") VALUES ('old', 'Old row');"#,
        )
        .unwrap();
    }

    let store = db.open();
    // A wire row carrying columns the old table lacked applies cleanly…
    store
        .apply_batch(
            issues(),
            &[insert(
                "a",
                json!({"id": "a", "title": "A", "status": "todo", "description": "d"}),
            )],
            None,
        )
        .unwrap();

    let rows = store.read_all(issues()).unwrap();
    assert_eq!(rows.len(), 2);
    let row = row_by_id(&rows, "a");
    assert_eq!(row.get("status").and_then(Value::as_str), Some("todo"));
    assert_eq!(row.get("description").and_then(Value::as_str), Some("d"));
    // …and the pre-existing row survives with the healed columns as NULL.
    let old = row_by_id(&rows, "old");
    assert_eq!(old.get("title").and_then(Value::as_str), Some("Old row"));
    assert_eq!(old.get("status"), Some(&Value::Null));

    // Healing must also stamp the shape's refetch marker: the healed columns
    // are NULL on every pre-existing row and incremental sync never backfills
    // them (Electric only sends rows that change), so the next poll must be a
    // full re-snapshot — the 0.8.4→0.8.5 vanishing-projects upgrade bug.
    let st = store.shape_state("issues").unwrap().unwrap();
    assert!(st.needs_refetch, "healed shape must force a re-snapshot");
    // A freshly created table (full column set from ddl()) heals nothing and
    // gets NO marker — a clean first open starts a plain initial snapshot.
    assert!(store.shape_state("projects").unwrap().is_none());
}

// ---------------------------------------------------------------------------
// §5.4 scalar binding round-trip
// ---------------------------------------------------------------------------

/// §5.4 CRITICAL round-trip: a string binds WITHOUT surrounding quotes, JSON
/// null becomes real SQL NULL (not the TEXT `null`), numbers keep their text
/// form, bools land as `t`/`f`.
#[test]
fn bind_scalars_not_json_blobs() {
    let db = TempDb::new();
    let store = db.open();

    store
        .apply_batch(
            issues(),
            &[insert(
                "row-1",
                json!({
                    "id": "row-1",
                    "title": "First issue",
                    "description": null,
                    "number": 1,
                    "sort_order": 1.5
                }),
            )],
            None,
        )
        .unwrap();

    let rows = store.read_all(issues()).unwrap();
    let row = row_by_id(&rows, "row-1");
    // NO quotes around the stored text.
    assert_eq!(row.get("title"), Some(&Value::String("First issue".into())));
    // Real SQL NULL, not the string "null".
    assert_eq!(row.get("description"), Some(&Value::Null));
    // Numbers pinned to their TEXT form (ONE canonical storage form, §5.5).
    assert_eq!(row.get("number"), Some(&Value::String("1".into())));
    assert_eq!(row.get("sort_order"), Some(&Value::String("1.5".into())));

    // Bools → canonical "t"/"f" (workspaces.is_public is a bool column).
    let workspaces = shape_by_name("workspaces").unwrap();
    store
        .apply_batch(
            workspaces,
            &[
                insert("w-1", json!({"id": "w-1", "name": "A", "is_public": true})),
                insert("w-2", json!({"id": "w-2", "name": "B", "is_public": false})),
            ],
            None,
        )
        .unwrap();
    let rows = store.read_all(workspaces).unwrap();
    assert_eq!(
        row_by_id(&rows, "w-1").get("is_public"),
        Some(&Value::String("t".into()))
    );
    assert_eq!(
        row_by_id(&rows, "w-2").get("is_public"),
        Some(&Value::String("f".into()))
    );
}

// ---------------------------------------------------------------------------
// §5.6c — 409 path: no flicker, atomic swap
// ---------------------------------------------------------------------------

#[test]
fn refetch_marker_keeps_stale_rows_readable() {
    let db = TempDb::new();
    let store = db.open();

    store
        .apply_batch(
            issues(),
            &[
                insert("a", json!({"id": "a", "title": "A"})),
                insert("b", json!({"id": "b", "title": "B"})),
                ShapeMessage::UpToDate,
            ],
            Some(&state("h-1", "0_0", true)),
        )
        .unwrap();

    // 409 arrives with a replacement handle: mark, do NOT delete.
    store.mark_needs_refetch("issues", Some("h-2")).unwrap();

    // Stale rows remain readable the whole time (the vanished-issues symptom was
    // exactly this window flashing empty).
    assert_eq!(store.count(issues()).unwrap(), 2);
    let st = store.shape_state("issues").unwrap().unwrap();
    assert_eq!(
        st,
        ShapeState {
            handle: "h-2".into(),
            offset: "-1".into(),
            needs_refetch: true,
            is_live: false,
        }
    );
    // The marker also survives a close/reopen (quit between 409 and refetch).
    drop(store);
    let store = db.open();
    assert_eq!(store.count(issues()).unwrap(), 2);
    assert!(store.shape_state("issues").unwrap().unwrap().needs_refetch);
}

#[test]
fn refetch_swap_is_atomic_delete_plus_inserts() {
    let db = TempDb::new();
    let store = db.open();

    store
        .apply_batch(
            issues(),
            &[
                insert("a", json!({"id": "a", "title": "A"})),
                insert("b", json!({"id": "b", "title": "B"})),
            ],
            Some(&state("h-1", "0_0", true)),
        )
        .unwrap();
    store.mark_needs_refetch("issues", Some("h-2")).unwrap();

    // The refetch snapshot: synthetic MustRefetch head + fresh rows + new
    // cursor, all in ONE transaction.
    store
        .apply_batch(
            issues(),
            &[
                ShapeMessage::MustRefetch,
                insert("c", json!({"id": "c", "title": "C"})),
                ShapeMessage::UpToDate,
            ],
            Some(&state("h-2", "0_0", true)),
        )
        .unwrap();

    let rows = store.read_all(issues()).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].get("id").and_then(Value::as_str), Some("c"));
    let st = store.shape_state("issues").unwrap().unwrap();
    assert!(!st.needs_refetch);
    assert!(st.is_live);
    assert_eq!(st.handle, "h-2");
}

/// All-or-nothing: a batch that fails mid-way must leave rows AND cursor
/// untouched (the offset may only advance with the rows it describes).
#[test]
fn failed_batch_rolls_back_rows_and_cursor() {
    let db = TempDb::new();
    let store = db.open();

    let s0 = state("h-1", "0_0", true);
    store
        .apply_batch(
            issues(),
            &[insert("a", json!({"id": "a", "title": "A"}))],
            Some(&s0),
        )
        .unwrap();

    // A composite key on an `id` table is a hard error → whole batch rolls back.
    let bad = ShapeMessage::Delete {
        key: RowKey::Pair("x".into(), "y".into()),
    };
    let result = store.apply_batch(
        issues(),
        &[
            insert("b", json!({"id": "b", "title": "B"})),
            bad,
        ],
        Some(&state("h-1", "0_9", true)),
    );
    assert!(result.is_err());

    // Neither the valid insert nor the cursor advance survived.
    let rows = store.read_all(issues()).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].get("id").and_then(Value::as_str), Some("a"));
    assert_eq!(store.shape_state("issues").unwrap().unwrap(), s0);
}

// ---------------------------------------------------------------------------
// Generic upsert / delete semantics
// ---------------------------------------------------------------------------

/// Partial updates (a value carrying a subset of columns) touch exactly the
/// present columns and leave the rest intact (Zig-style generic upsert).
#[test]
fn partial_update_preserves_absent_columns() {
    let db = TempDb::new();
    let store = db.open();

    store
        .apply_batch(
            issues(),
            &[insert(
                "a",
                json!({"id": "a", "title": "A", "status": "todo", "priority": "high"}),
            )],
            None,
        )
        .unwrap();
    store
        .apply_batch(
            issues(),
            &[ShapeMessage::Update {
                key: RowKey::Single("a".into()),
                value: json!({"id": "a", "title": "A (renamed)"})
                    .as_object()
                    .cloned()
                    .unwrap(),
            }],
            None,
        )
        .unwrap();

    let rows = store.read_all(issues()).unwrap();
    let row = row_by_id(&rows, "a");
    assert_eq!(row.get("title").and_then(Value::as_str), Some("A (renamed)"));
    assert_eq!(row.get("status").and_then(Value::as_str), Some("todo"));
    assert_eq!(row.get("priority").and_then(Value::as_str), Some("high"));
}

/// The PK is injected from the row key when the value object lacks it, and
/// deletes never rely on the value at all (§5.2).
#[test]
fn pk_comes_from_key_when_value_lacks_it() {
    let db = TempDb::new();
    let store = db.open();

    store
        .apply_batch(
            issues(),
            &[ShapeMessage::Update {
                key: RowKey::Single("k-1".into()),
                value: json!({"title": "Keyed"}).as_object().cloned().unwrap(),
            }],
            None,
        )
        .unwrap();
    let rows = store.read_all(issues()).unwrap();
    assert_eq!(rows[0].get("id").and_then(Value::as_str), Some("k-1"));

    store
        .apply_batch(
            issues(),
            &[ShapeMessage::Delete {
                key: RowKey::Single("k-1".into()),
            }],
            None,
        )
        .unwrap();
    assert_eq!(store.count(issues()).unwrap(), 0);
}

/// issue_labels — the ONLY composite-PK, id-less table — upserts and deletes
/// by the (issue_id, label_id) pair.
#[test]
fn composite_pk_issue_labels() {
    let db = TempDb::new();
    let store = db.open();
    let spec = shape_by_name("issue_labels").unwrap();

    let key = RowKey::Pair("iss-1".into(), "lab-1".into());
    store
        .apply_batch(
            spec,
            &[ShapeMessage::Insert {
                key: key.clone(),
                value: json!({"issue_id": "iss-1", "label_id": "lab-1", "workspace_id": "w-1"})
                    .as_object()
                    .cloned()
                    .unwrap(),
            }],
            None,
        )
        .unwrap();
    assert_eq!(store.count(spec).unwrap(), 1);

    // Upsert (same pair) does not duplicate.
    store
        .apply_batch(
            spec,
            &[ShapeMessage::Insert {
                key: key.clone(),
                value: json!({"issue_id": "iss-1", "label_id": "lab-1", "workspace_id": "w-2"})
                    .as_object()
                    .cloned()
                    .unwrap(),
            }],
            None,
        )
        .unwrap();
    let rows = store.read_all(spec).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].get("workspace_id").and_then(Value::as_str), Some("w-2"));

    store
        .apply_batch(spec, &[ShapeMessage::Delete { key }], None)
        .unwrap();
    assert_eq!(store.count(spec).unwrap(), 0);
}

// ---------------------------------------------------------------------------
// §5.5 hydrate coercion over the real fixtures
// ---------------------------------------------------------------------------

/// Hydrate test over the snake/camel fixtures (§5.5): the TEXT-pinned columns
/// coerce back to native numerics through the tolerant deserializers.
#[test]
fn hydrate_coerces_text_to_native() {
    #[derive(Deserialize)]
    struct IssueLite {
        id: String,
        #[serde(deserialize_with = "tolerant_i64")]
        number: i64,
        #[serde(deserialize_with = "tolerant_opt_f64")]
        sort_order: Option<f64>,
        assignee_id: Option<String>,
        due_date: Option<String>,
    }

    let db = TempDb::new();
    let store = db.open();
    store
        .apply_batch(issues(), &[fixture_message("snake-case.json")], None)
        .unwrap();

    let rows = store.read_all(issues()).unwrap();
    let issue: IssueLite =
        serde_json::from_value(Value::Object(rows[0].clone())).expect("tolerant hydrate");
    assert_eq!(issue.id, "01J9K0A0X3CB4E5F6G7H8J9K0L");
    assert_eq!(issue.number, 1); // "1" → 1
    assert_eq!(issue.sort_order, Some(1.0)); // "1.0"/"1" → 1.0
    assert_eq!(issue.assignee_id, None); // SQL NULL → None
    assert_eq!(issue.due_date.as_deref(), Some("2026-05-20"));
}
