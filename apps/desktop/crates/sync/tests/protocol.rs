//! Fixture conformance suite (masterplan-v3 §5.2 / Phase-2 gate bullet #1):
//! runs ALL `packages/electric-protocol/fixtures/*.json` against
//! `sync::protocol` with the same assertions the Swift/Zig engines were
//! tested against. These vectors are the cross-platform contract — do NOT add
//! Rust-only fixtures; extend `packages/electric-protocol/` if the protocol
//! genuinely grows.

use std::path::PathBuf;

use serde_json::{Map, Value};
use sync::protocol::{
    parse_messages, RowKey, ShapeMessage, ShapeResponseHeaders,
};

/// `packages/electric-protocol/fixtures/`, pathed relative to
/// `CARGO_MANIFEST_DIR` (apps/desktop/crates/sync) up to the repo root.
fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../../packages/electric-protocol/fixtures")
}

fn load_fixture(name: &str) -> Value {
    let path = fixtures_dir().join(name);
    let bytes = std::fs::read(&path)
        .unwrap_or_else(|e| panic!("fixture {} unreadable: {e}", path.display()));
    serde_json::from_slice(&bytes)
        .unwrap_or_else(|e| panic!("fixture {name} is not valid JSON: {e}"))
}

/// The response body of a request/response fixture, re-serialized to bytes
/// exactly as a transport would hand them to `parse_messages`.
fn response_body_bytes(fixture: &Value) -> Vec<u8> {
    let body = fixture
        .pointer("/response/body")
        .expect("fixture has response.body");
    serde_json::to_vec(body).expect("body serializes")
}

/// Response headers of a fixture as (name, value) pairs.
fn response_headers(fixture: &Value) -> ShapeResponseHeaders {
    let headers = fixture
        .pointer("/response/headers")
        .and_then(Value::as_object)
        .expect("fixture has response.headers");
    ShapeResponseHeaders::from_pairs(
        headers
            .iter()
            .filter_map(|(k, v)| v.as_str().map(|v| (k.as_str(), v))),
    )
}

// --- §5.2 assertion 1: initial-snapshot.json -------------------------------

#[test]
fn initial_snapshot_two_inserts_and_up_to_date() {
    let fixture = load_fixture("initial-snapshot.json");
    let msgs = parse_messages(&response_body_bytes(&fixture), false);

    assert_eq!(msgs.len(), 3);
    let ShapeMessage::Insert { key, value } = &msgs[0] else {
        panic!("message 0 must be an insert: {:?}", msgs[0]);
    };
    assert_eq!(key, &RowKey::Single("01J9K0A0X3CB4E5F6G7H8J9K0L".into()));
    // Values arrive snake_case and stay snake_case.
    assert_eq!(
        value.get("project_id").and_then(Value::as_str),
        Some("01J9K0A0X3CB4E5F6G7H8J9K0M")
    );
    assert_eq!(
        value.get("created_at").and_then(Value::as_str),
        Some("2025-12-01T12:00:00Z")
    );
    assert!(!value.contains_key("projectId"));
    assert_eq!(value.get("title").and_then(Value::as_str), Some("First issue"));
    // Heterogeneous scalars stay raw at parse time (§5.5): number is bare 1.
    assert_eq!(value.get("number"), Some(&Value::from(1)));

    let ShapeMessage::Insert { key, .. } = &msgs[1] else {
        panic!("message 1 must be an insert: {:?}", msgs[1]);
    };
    assert_eq!(key, &RowKey::Single("01J9K0A0X3CB4E5F6G7H8J9K0N".into()));

    assert_eq!(msgs[2], ShapeMessage::UpToDate);

    // Response-header extraction drives the next request.
    let headers = response_headers(&fixture);
    assert_eq!(headers.handle.as_deref(), Some("h-2c3e9f10"));
    assert_eq!(headers.offset.as_deref(), Some("0_0"));
    assert_eq!(headers.cursor, None);
}

// --- §5.2 assertion 2: live-update.json ------------------------------------

#[test]
fn live_update_update_delete_up_to_date() {
    let fixture = load_fixture("live-update.json");
    let msgs = parse_messages(&response_body_bytes(&fixture), false);

    assert_eq!(msgs.len(), 3);
    let ShapeMessage::Update { key, value } = &msgs[0] else {
        panic!("message 0 must be an update: {:?}", msgs[0]);
    };
    assert_eq!(key, &RowKey::Single("01J9K0A0X3CB4E5F6G7H8J9K0L".into()));
    assert_eq!(
        value.get("title").and_then(Value::as_str),
        Some("First issue (renamed)")
    );

    // The delete key parses to …0N; the (partial) value is never relied on.
    let ShapeMessage::Delete { key } = &msgs[1] else {
        panic!("message 1 must be a delete: {:?}", msgs[1]);
    };
    assert_eq!(key, &RowKey::Single("01J9K0A0X3CB4E5F6G7H8J9K0N".into()));

    assert_eq!(msgs[2], ShapeMessage::UpToDate);

    let headers = response_headers(&fixture);
    assert_eq!(headers.offset.as_deref(), Some("0_1"));
}

// --- §5.2 assertion 3: must-refetch.json ------------------------------------

#[test]
fn must_refetch_is_a_lone_control() {
    let fixture = load_fixture("must-refetch.json");
    let msgs = parse_messages(&response_body_bytes(&fixture), false);
    assert_eq!(msgs, vec![ShapeMessage::MustRefetch]);
}

// --- §5.2 assertion 4: up-to-date.json --------------------------------------

#[test]
fn up_to_date_is_a_lone_control() {
    let fixture = load_fixture("up-to-date.json");
    let msgs = parse_messages(&response_body_bytes(&fixture), false);
    assert_eq!(msgs, vec![ShapeMessage::UpToDate]);
}

// --- §5.2 assertion 5: camel-case.json ≡ snake-case.json ---------------------

/// The single-message fixtures are one message object each (plus a top-level
/// documentation `description` string the parser ignores); wrap in an array.
fn parse_single_message_fixture(name: &str) -> Map<String, Value> {
    let fixture = load_fixture(name);
    let body = serde_json::to_vec(&Value::Array(vec![fixture])).unwrap();
    let msgs = parse_messages(&body, false);
    assert_eq!(msgs.len(), 1, "{name} decodes to exactly one message");
    let ShapeMessage::Insert { key, value } = msgs.into_iter().next().unwrap() else {
        panic!("{name} must decode to an insert");
    };
    assert_eq!(key, RowKey::Single("01J9K0A0X3CB4E5F6G7H8J9K0L".into()));
    value
}

#[test]
fn camel_and_snake_fixtures_normalize_byte_identically() {
    let camel = parse_single_message_fixture("camel-case.json");
    let snake = parse_single_message_fixture("snake-case.json");

    // Byte-identical normalized maps (§5.2 assertion 5).
    assert_eq!(camel, snake);
    assert_eq!(
        serde_json::to_string(&camel).unwrap(),
        serde_json::to_string(&snake).unwrap()
    );

    // Spot-check the normalization actually happened.
    assert!(snake.contains_key("project_id"));
    assert!(camel.contains_key("project_id"));
    assert!(!camel.contains_key("projectId"));
    assert_eq!(camel.get("sort_order"), Some(&Value::from(1.0)));

    // The stale pre-GFM fields ARE modeled as ordinary keys by the parser
    // (the STORE drops them via the known-column allowlist — see
    // tests/store.rs; do not "fix" the fixture by hiding them here).
    assert_eq!(camel.get("due_time"), Some(&Value::Null));
    assert_eq!(camel.get("end_time"), Some(&Value::Null));
    assert!(camel.get("description").unwrap().is_object());
}

// --- §5.2 assertion 6: empty bodies ------------------------------------------

#[test]
fn empty_bodies_decode_to_zero_messages() {
    assert_eq!(parse_messages(b"[]", false), vec![]);
    assert_eq!(parse_messages(b"", false), vec![]);
}
