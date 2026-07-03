//! The Electric shape wire protocol — PURE and fixture-locked (masterplan-v3
//! §5.2). Nothing here may `use gpui` or perform I/O; `client.rs` drives these
//! functions from its blocking long-poll loop and `store.rs` consumes the
//! decoded [`ShapeMessage`] batches.
//!
//! The single source of protocol truth is `packages/electric-protocol/`
//! (README + JSON fixtures); `tests/protocol.rs` runs every fixture against
//! this module. The wire contract is shared by web (`@electric-sql/client`),
//! iOS/macOS (`ShapeClient.swift`) and Android — this is the fourth consumer
//! of the same contract, not a new dialect.
//!
//! Load-bearing rule (§5.2): the client sends ONLY `offset`, `handle`, `live`
//! and (belt-and-suspenders) `cursor`. It NEVER sends `where` or `columns` —
//! the server proxies pin both (`createShapeRouteHandler`), which is how we
//! inherit the sorted-where shape-identity fix (EXP-1 #13d) and the
//! `issue_subscribers` email-PII exclusion for free.

use serde_json::{Map, Value};

/// The initial-snapshot offset — the very first request for a shape (and every
/// post-409 refetch) goes out as `?offset=-1`.
pub const INITIAL_OFFSET: &str = "-1";

/// Response header carrying the shape handle for the *next* request.
pub const HEADER_HANDLE: &str = "electric-handle";
/// Response header carrying the offset for the *next* request.
pub const HEADER_OFFSET: &str = "electric-offset";
/// Optional response header echoed back as `?cursor=` (transient cache-buster).
pub const HEADER_CURSOR: &str = "electric-cursor";
/// Optional response header describing the shape's column schema (unused, but
/// extracted so the client can log it).
pub const HEADER_SCHEMA: &str = "electric-schema";

// ---------------------------------------------------------------------------
// Row keys
// ---------------------------------------------------------------------------

/// A parsed Electric row key. The wire form is a slash-separated list of
/// double-quoted segments, e.g. `"issues"/"01J9…0L"` or (schema-qualified)
/// `"public"."issue_labels"/"<issue_id>"/"<label_id>"`. Leading segments are
/// metadata we don't need — we already know the table from the request — so
/// only the trailing 1 (normal `id` PK) or 2 (`issue_labels` composite PK)
/// segments matter (§5.2).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum RowKey {
    /// Single-column `id` primary key.
    Single(String),
    /// Composite `(issue_id, label_id)` primary key — `issue_labels` only.
    Pair(String, String),
}

impl RowKey {
    /// Parse a raw wire key. `composite` selects the trailing-2-segment form
    /// (`issue_labels`); everything else takes the trailing segment. Returns
    /// `None` on malformed keys (tolerate-and-drop, never panic).
    pub fn parse(raw: &str, composite: bool) -> Option<RowKey> {
        if raw.is_empty() {
            return None;
        }
        let segs: Vec<&str> = raw.split('/').map(|s| s.trim_matches('"')).collect();
        if composite {
            if segs.len() < 2 {
                return None;
            }
            let n = segs.len();
            Some(RowKey::Pair(segs[n - 2].to_string(), segs[n - 1].to_string()))
        } else {
            let last = *segs.last()?;
            if last.is_empty() {
                return None;
            }
            Some(RowKey::Single(last.to_string()))
        }
    }
}

// ---------------------------------------------------------------------------
// Wire messages
// ---------------------------------------------------------------------------

/// One decoded message of a shape response body (§5.2). `insert` and `update`
/// are both upserts — the store treats them identically; we keep the variant
/// only for logging parity with iOS/Zig. `delete` carries no value — the id
/// always comes from the key (the wire `value` may be absent or partial).
#[derive(Debug, Clone, PartialEq)]
pub enum ShapeMessage {
    Insert { key: RowKey, value: Map<String, Value> },
    Update { key: RowKey, value: Map<String, Value> },
    Delete { key: RowKey },
    UpToDate,
    MustRefetch,
}

/// Decode a shape response body into messages.
///
/// * Empty / whitespace-only / `[]` bodies → 0 messages (no panic).
/// * A malformed top-level body → 0 messages (tolerant, mirrors iOS
///   `decodeMessages`).
/// * Unknown controls and operations are skipped; `snapshot-end` is recognized
///   as a chunk boundary and dropped (liveness is gated on `up-to-date`,
///   never on `snapshot-end` — iOS parity).
/// * Every `value` key is normalized camelCase→snake_case at parse time
///   (§5.2): the store only ever sees snake_case column names. Values stay
///   raw [`serde_json::Value`]s — Electric delivers heterogeneous scalars
///   (strings, bare numbers, bools, nulls, the occasional nested object) and
///   coercion happens at bind (§5.4) / hydrate (§5.5) time, never here.
///
/// `composite_keys` selects the trailing-2-segment key form — `true` only for
/// `issue_labels` (see `shapes.rs`).
pub fn parse_messages(body: &[u8], composite_keys: bool) -> Vec<ShapeMessage> {
    if body.iter().all(|b| b.is_ascii_whitespace()) {
        return Vec::new();
    }
    let Ok(Value::Array(items)) = serde_json::from_slice::<Value>(body) else {
        return Vec::new();
    };
    items
        .into_iter()
        .filter_map(|item| parse_message(item, composite_keys))
        .collect()
}

fn parse_message(item: Value, composite_keys: bool) -> Option<ShapeMessage> {
    let Value::Object(mut obj) = item else {
        return None;
    };
    let headers = match obj.get("headers") {
        Some(Value::Object(h)) => h.clone(),
        _ => Map::new(),
    };

    if let Some(Value::String(control)) = headers.get("control") {
        return match control.as_str() {
            "up-to-date" => Some(ShapeMessage::UpToDate),
            "must-refetch" => Some(ShapeMessage::MustRefetch),
            // Chunk boundary of a multi-response snapshot — recognized but
            // carries no data (iOS parity).
            "snapshot-end" => None,
            _ => None,
        };
    }

    let operation = match headers.get("operation") {
        Some(Value::String(op)) => op.clone(),
        _ => return None,
    };
    let key = match obj.get("key") {
        Some(Value::String(k)) => RowKey::parse(k, composite_keys)?,
        _ => return None,
    };

    match operation.as_str() {
        "insert" | "update" => {
            let Some(Value::Object(value)) = obj.remove("value") else {
                // An upsert without a value object is meaningless — drop it.
                return None;
            };
            let value = normalize_keys(value);
            if operation == "insert" {
                Some(ShapeMessage::Insert { key, value })
            } else {
                Some(ShapeMessage::Update { key, value })
            }
        }
        // Never rely on a delete's `value` — the id comes from the key (§5.2).
        "delete" => Some(ShapeMessage::Delete { key }),
        _ => None,
    }
}

/// Normalize every key of a wire `value` object from camelCase to snake_case.
fn normalize_keys(value: Map<String, Value>) -> Map<String, Value> {
    value
        .into_iter()
        .map(|(k, v)| (camel_to_snake(&k), v))
        .collect()
}

/// Tiny hand-rolled camelCase→snake_case (§5.2 — deliberately NOT `heck`; it
/// must match the Swift/Zig behaviour exactly, including all-lowercase keys
/// passing through untouched): every ASCII uppercase becomes `_<lower>`,
/// except at the start of the string.
pub fn camel_to_snake(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 4);
    for (i, ch) in s.chars().enumerate() {
        if ch.is_ascii_uppercase() {
            if i > 0 {
                out.push('_');
            }
            out.push(ch.to_ascii_lowercase());
        } else {
            out.push(ch);
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Response headers
// ---------------------------------------------------------------------------

/// The Electric response headers that drive the next request (§5.2).
/// `handle`/`offset` are persisted only after a batch applies cleanly (§5.4);
/// `cursor` is stored transiently for the `?cursor=` echo; `schema` is
/// informational.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ShapeResponseHeaders {
    pub handle: Option<String>,
    pub offset: Option<String>,
    pub cursor: Option<String>,
    pub schema: Option<String>,
}

impl ShapeResponseHeaders {
    /// Extract from any transport's `(name, value)` pairs — header names are
    /// matched case-insensitively.
    pub fn from_pairs<'a, I>(pairs: I) -> Self
    where
        I: IntoIterator<Item = (&'a str, &'a str)>,
    {
        let mut out = Self::default();
        for (name, value) in pairs {
            let slot = if name.eq_ignore_ascii_case(HEADER_HANDLE) {
                &mut out.handle
            } else if name.eq_ignore_ascii_case(HEADER_OFFSET) {
                &mut out.offset
            } else if name.eq_ignore_ascii_case(HEADER_CURSOR) {
                &mut out.cursor
            } else if name.eq_ignore_ascii_case(HEADER_SCHEMA) {
                &mut out.schema
            } else {
                continue;
            };
            *slot = Some(value.to_string());
        }
        out
    }
}

// ---------------------------------------------------------------------------
// Per-shape cursor state machine (initial -1 → snapshot → live)
// ---------------------------------------------------------------------------

/// The persisted per-shape cursor state (one `electric_offsets` row, §5.4).
/// Direct port of iOS `ElectricOffset` — `needs_refetch` survives a quit
/// between a 409 and the refetch so the atomic DELETE+reinsert still happens
/// after relaunch; `is_live` gates the `live=true` long-poll (only flipped by
/// `up-to-date`, never by `snapshot-end`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShapeState {
    /// Shape handle. May be empty (`""`) after an inline `must-refetch`, where
    /// the old handle is dead and Electric sent no replacement.
    pub handle: String,
    pub offset: String,
    /// A 409 / inline `must-refetch` was seen; the next poll is an
    /// `offset=-1` re-snapshot whose batch gets a synthetic
    /// [`ShapeMessage::MustRefetch`] prepended (§5.6c).
    pub needs_refetch: bool,
    /// `up-to-date` has been seen: the loop long-polls with `live=true`.
    /// Catch-up polls stay non-live per the Electric protocol.
    pub is_live: bool,
}

impl ShapeState {
    /// The state persisted on the 409 path (§5.6c step 1): keep the
    /// replacement handle Electric sent in the 409 response header (when
    /// present), reset the offset to `-1`, and mark the refetch — WITHOUT
    /// touching any table rows (stale rows stay visible until the refetch
    /// batch replaces them atomically).
    pub fn refetch_marker(replacement_handle: Option<&str>) -> ShapeState {
        ShapeState {
            handle: replacement_handle.unwrap_or_default().to_string(),
            offset: INITIAL_OFFSET.to_string(),
            needs_refetch: true,
            is_live: false,
        }
    }

    /// The state to persist after a batch applied cleanly (direct port of
    /// `ShapeClient.pollOnce`): adopt the response's `electric-handle` /
    /// `electric-offset`, clear `needs_refetch`, and compute liveness as
    /// `saw_up_to_date || (was_live && !was_refetching)`. Returns `None` when
    /// the response carried no handle/offset pair — nothing to persist, the
    /// next poll re-requests the same cursor (at-least-once + idempotent
    /// upserts).
    pub fn after_apply(
        headers: &ShapeResponseHeaders,
        prev: Option<&ShapeState>,
        saw_up_to_date: bool,
    ) -> Option<ShapeState> {
        let (Some(handle), Some(offset)) = (&headers.handle, &headers.offset) else {
            return None;
        };
        let was_live = prev.is_some_and(|s| s.is_live);
        let was_refetching = prev.is_some_and(|s| s.needs_refetch);
        Some(ShapeState {
            handle: handle.clone(),
            offset: offset.clone(),
            needs_refetch: false,
            is_live: saw_up_to_date || (was_live && !was_refetching),
        })
    }
}

/// Build the query params for the next poll from the persisted state (§5.2
/// request shape; direct port of `ShapeClient.pollOnce`):
///
/// * no saved state, or `needs_refetch` → initial snapshot / post-409 refetch:
///   `offset=-1`, plus the replacement `handle` when we hold one;
/// * otherwise → `offset={saved}&handle={saved}`, plus `live=true` once the
///   shape has reached head (`is_live`), plus the transient `cursor` echo on
///   live polls when the previous response carried one.
///
/// NEVER adds `where` or `columns` (§5.2 — load-bearing, see module docs).
pub fn request_params(
    saved: Option<&ShapeState>,
    cursor: Option<&str>,
) -> Vec<(&'static str, String)> {
    match saved {
        None => vec![("offset", INITIAL_OFFSET.to_string())],
        Some(state) if state.needs_refetch => {
            let mut params = vec![("offset", INITIAL_OFFSET.to_string())];
            if !state.handle.is_empty() {
                params.push(("handle", state.handle.clone()));
            }
            params
        }
        Some(state) => {
            let mut params = vec![
                ("offset", state.offset.clone()),
                ("handle", state.handle.clone()),
            ];
            if state.is_live {
                params.push(("live", "true".to_string()));
                if let Some(cursor) = cursor {
                    params.push(("cursor", cursor.to_string()));
                }
            }
            params
        }
    }
}

/// Assemble the full request URL: `{base}{path}?{params}` with percent-encoded
/// values. `base` must not end with `/` when `path` starts with one (the
/// shapes registry uses absolute `/api/shapes/…` paths).
pub fn build_url(base: &str, path: &str, params: &[(&'static str, String)]) -> String {
    let mut url = String::with_capacity(base.len() + path.len() + 64);
    url.push_str(base.strip_suffix('/').unwrap_or(base));
    url.push_str(path);
    for (i, (name, value)) in params.iter().enumerate() {
        url.push(if i == 0 { '?' } else { '&' });
        url.push_str(name);
        url.push('=');
        url.push_str(&percent_encode(value));
    }
    url
}

/// Minimal RFC-3986 percent-encoding of a query value (unreserved characters
/// pass through). Offsets/handles/cursors are URL-safe in practice; this is
/// belt-and-suspenders, not a general-purpose encoder.
fn percent_encode(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Batch helpers (the §5.6c dance, pure part)
// ---------------------------------------------------------------------------

/// Strip inline `must-refetch` control messages from a decoded batch (§5.6c:
/// Electric can signal a rotation inside an otherwise-200 body). Returns the
/// remaining messages and whether any `must-refetch` was present. The caller
/// applies the remaining messages, persists [`ShapeState::refetch_marker`]
/// (with NO replacement handle — the inline case carries none), and treats
/// the next poll as a refetch.
pub fn strip_inline_must_refetch(msgs: Vec<ShapeMessage>) -> (Vec<ShapeMessage>, bool) {
    let before = msgs.len();
    let remaining: Vec<ShapeMessage> = msgs
        .into_iter()
        .filter(|m| !matches!(m, ShapeMessage::MustRefetch))
        .collect();
    let had = remaining.len() != before;
    (remaining, had)
}

/// Whether a batch contains the `up-to-date` control — the ONLY thing that
/// flips a shape live (§5.2; `snapshot-end` never does).
pub fn contains_up_to_date(msgs: &[ShapeMessage]) -> bool {
    msgs.iter().any(|m| matches!(m, ShapeMessage::UpToDate))
}

/// Whether a RAW response body carries a `snapshot-end` control.
///
/// [`parse_messages`] deliberately drops `snapshot-end` (it never flips a
/// shape live), so the refetch path needs this raw-body scan to tell a
/// **genuinely empty snapshot** apart from an empty/malformed body: live
/// Electric (1.6.9) answers `offset=-1` for a zero-row shape with a LONE
/// `snapshot-end` control — no rows, no `up-to-date` (that only arrives on
/// the follow-up poll). Verified against the runtime gate, 2026-07-03.
pub fn contains_snapshot_end(body: &[u8]) -> bool {
    let Ok(Value::Array(items)) = serde_json::from_slice::<Value>(body) else {
        return false;
    };
    items.iter().any(|item| {
        matches!(
            item.get("headers").and_then(|h| h.get("control")),
            Some(Value::String(control)) if control == "snapshot-end"
        )
    })
}

// ---------------------------------------------------------------------------
// Unit tests (the fixture suite lives in tests/protocol.rs)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn camel_to_snake_matches_contract() {
        assert_eq!(camel_to_snake("projectId"), "project_id");
        assert_eq!(camel_to_snake("createdAt"), "created_at");
        assert_eq!(camel_to_snake("prMergedAt"), "pr_merged_at");
        // All-lowercase keys pass through untouched (§5.2).
        assert_eq!(camel_to_snake("id"), "id");
        assert_eq!(camel_to_snake("sort_order"), "sort_order");
        // Leading uppercase does not grow a leading underscore.
        assert_eq!(camel_to_snake("Id"), "id");
    }

    #[test]
    fn key_parsing_single_and_composite() {
        assert_eq!(
            RowKey::parse("\"issues\"/\"01J9K0A0X3CB4E5F6G7H8J9K0L\"", false),
            Some(RowKey::Single("01J9K0A0X3CB4E5F6G7H8J9K0L".into()))
        );
        // Schema-qualified first segment is metadata we don't need.
        assert_eq!(
            RowKey::parse("\"public\".\"issues\"/\"abc\"", false),
            Some(RowKey::Single("abc".into()))
        );
        assert_eq!(
            RowKey::parse("\"issue_labels\"/\"iss-1\"/\"lab-1\"", true),
            Some(RowKey::Pair("iss-1".into(), "lab-1".into()))
        );
        assert_eq!(RowKey::parse("", false), None);
        assert_eq!(RowKey::parse("\"only-one\"", true), None);
    }

    #[test]
    fn request_params_initial_snapshot() {
        assert_eq!(
            request_params(None, None),
            vec![("offset", "-1".to_string())]
        );
        // A stray cursor never leaks onto a snapshot request.
        assert_eq!(
            request_params(None, Some("c-1")),
            vec![("offset", "-1".to_string())]
        );
    }

    #[test]
    fn request_params_refetch_carries_replacement_handle() {
        let state = ShapeState::refetch_marker(Some("h-new"));
        assert_eq!(
            request_params(Some(&state), None),
            vec![
                ("offset", "-1".to_string()),
                ("handle", "h-new".to_string())
            ]
        );
        // Inline must-refetch case: no replacement handle → bare offset=-1.
        let state = ShapeState::refetch_marker(None);
        assert_eq!(
            request_params(Some(&state), None),
            vec![("offset", "-1".to_string())]
        );
    }

    #[test]
    fn request_params_catch_up_stays_non_live() {
        let state = ShapeState {
            handle: "h-1".into(),
            offset: "0_0".into(),
            needs_refetch: false,
            is_live: false,
        };
        assert_eq!(
            request_params(Some(&state), Some("c-1")),
            vec![
                ("offset", "0_0".to_string()),
                ("handle", "h-1".to_string()),
            ]
        );
    }

    #[test]
    fn request_params_live_long_poll_with_cursor_echo() {
        let state = ShapeState {
            handle: "h-1".into(),
            offset: "0_5".into(),
            needs_refetch: false,
            is_live: true,
        };
        assert_eq!(
            request_params(Some(&state), Some("c-9")),
            vec![
                ("offset", "0_5".to_string()),
                ("handle", "h-1".to_string()),
                ("live", "true".to_string()),
                ("cursor", "c-9".to_string()),
            ]
        );
    }

    #[test]
    fn build_url_encodes_values() {
        let params = vec![("offset", "-1".to_string()), ("handle", "h 1/2".to_string())];
        assert_eq!(
            build_url("http://localhost:5173", "/api/shapes/issues", &params),
            "http://localhost:5173/api/shapes/issues?offset=-1&handle=h%201%2F2"
        );
        assert_eq!(
            build_url("http://x/", "/api/shapes/issues", &[]),
            "http://x/api/shapes/issues"
        );
    }

    #[test]
    fn header_extraction_is_case_insensitive() {
        let headers = ShapeResponseHeaders::from_pairs([
            ("Electric-Handle", "h-1"),
            ("ELECTRIC-OFFSET", "0_0"),
            ("electric-cursor", "c-1"),
            ("electric-schema", "{}"),
            ("content-type", "application/json"),
        ]);
        assert_eq!(
            headers,
            ShapeResponseHeaders {
                handle: Some("h-1".into()),
                offset: Some("0_0".into()),
                cursor: Some("c-1".into()),
                schema: Some("{}".into()),
            }
        );
    }

    #[test]
    fn after_apply_liveness_rule() {
        let headers = ShapeResponseHeaders {
            handle: Some("h-2".into()),
            offset: Some("0_9".into()),
            cursor: None,
            schema: None,
        };
        // up-to-date flips live from cold.
        let next = ShapeState::after_apply(&headers, None, true).unwrap();
        assert!(next.is_live && !next.needs_refetch);
        // A live shape stays live across a delta without up-to-date…
        let live = ShapeState {
            handle: "h-1".into(),
            offset: "0_0".into(),
            needs_refetch: false,
            is_live: true,
        };
        assert!(ShapeState::after_apply(&headers, Some(&live), false).unwrap().is_live);
        // …but a refetch snapshot does NOT stay live until up-to-date arrives.
        let refetching = ShapeState::refetch_marker(Some("h-2"));
        let next = ShapeState::after_apply(&headers, Some(&refetching), false).unwrap();
        assert!(!next.is_live);
        assert!(!next.needs_refetch);
        // Missing headers → nothing to persist.
        assert_eq!(
            ShapeState::after_apply(&ShapeResponseHeaders::default(), None, true),
            None
        );
    }

    #[test]
    fn strip_inline_must_refetch_keeps_other_messages() {
        let msgs = vec![
            ShapeMessage::UpToDate,
            ShapeMessage::MustRefetch,
            ShapeMessage::Delete {
                key: RowKey::Single("x".into()),
            },
        ];
        let (remaining, had) = strip_inline_must_refetch(msgs);
        assert!(had);
        assert_eq!(remaining.len(), 2);
        assert!(!remaining.iter().any(|m| matches!(m, ShapeMessage::MustRefetch)));

        let (remaining, had) = strip_inline_must_refetch(vec![ShapeMessage::UpToDate]);
        assert!(!had);
        assert_eq!(remaining.len(), 1);
    }

    #[test]
    fn parse_tolerates_garbage_and_unknowns() {
        assert_eq!(parse_messages(b"", false), vec![]);
        assert_eq!(parse_messages(b"   ", false), vec![]);
        assert_eq!(parse_messages(b"[]", false), vec![]);
        assert_eq!(parse_messages(b"not json", false), vec![]);
        assert_eq!(parse_messages(b"{\"headers\":{}}", false), vec![]); // not an array
        // Unknown control + unknown operation + snapshot-end are all skipped.
        let body = br#"[
            {"headers": {"control": "snapshot-end"}},
            {"headers": {"control": "future-control"}},
            {"headers": {"operation": "upsert"}, "key": "\"t\"/\"a\"", "value": {}},
            {"headers": {"operation": "insert"}, "key": "\"t\"/\"a\""}
        ]"#;
        assert_eq!(parse_messages(body, false), vec![]);
    }

    #[test]
    fn parse_preserves_heterogeneous_scalars_raw() {
        // §5.5: strings, bare numbers, bools, nulls and nested objects all
        // survive parsing untouched — coercion is bind/hydrate-time only.
        let body = br#"[{
            "headers": {"operation": "insert"},
            "key": "\"issues\"/\"a\"",
            "value": {"id": "a", "number": 1, "sortOrder": 1.5,
                      "flag": true, "assigneeId": null,
                      "description": {"text": "x"}}
        }]"#;
        let msgs = parse_messages(body, false);
        assert_eq!(msgs.len(), 1);
        let ShapeMessage::Insert { value, .. } = &msgs[0] else {
            panic!("expected insert");
        };
        assert_eq!(value.get("number"), Some(&Value::from(1)));
        assert_eq!(value.get("sort_order"), Some(&Value::from(1.5)));
        assert_eq!(value.get("flag"), Some(&Value::Bool(true)));
        assert_eq!(value.get("assignee_id"), Some(&Value::Null));
        assert!(value.get("description").unwrap().is_object());
    }

    #[test]
    fn contains_snapshot_end_distinguishes_empty_snapshot_from_empty_body() {
        // The wire form live Electric (1.6.9) sends for a zero-row shape's
        // `offset=-1` snapshot: a lone snapshot-end, no rows, no up-to-date.
        let lone = br#"[{"headers":{"control":"snapshot-end","xip_list":[],"xmax":"914","xmin":"914"}}]"#;
        assert!(contains_snapshot_end(lone));
        // parse_messages drops it — hence the dedicated raw-body scan.
        assert!(parse_messages(lone, false).is_empty());

        // Among rows, still detected.
        let mixed = br#"[{"headers":{"operation":"insert"},"key":"\"issues\"/\"a\"","value":{"id":"a"}},{"headers":{"control":"snapshot-end"}}]"#;
        assert!(contains_snapshot_end(mixed));

        // Empty / malformed / plain-empty-array bodies: NOT a snapshot end.
        assert!(!contains_snapshot_end(b""));
        assert!(!contains_snapshot_end(b"[]"));
        assert!(!contains_snapshot_end(b"not json"));
        assert!(!contains_snapshot_end(br#"[{"headers":{"control":"up-to-date"}}]"#));
    }
}
