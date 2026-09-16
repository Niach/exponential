//! EXP-879 — a coding run's PUBLISHED RESULTS: the pictures an agent files
//! while it works, read off `coding_sessions.results`.
//!
//! The column is a flat, ORDERED jsonb array of
//! `{topic, label, attachmentId, width, height}` — camelCase INSIDE the blob
//! (it is a document, not a row), `null`/`[]` = nothing published. This
//! module is the ONE reader on the desktop, the twin of the web
//! `lib/session-results.ts` and of the iOS/Android helpers: every client
//! parses the SAME way, so a blob one of them would drop is dropped
//! everywhere.
//!
//! Tolerance is the point. The blob is written by an agent through MCP and
//! hydrated out of SQLite (where a jsonb column arrives as a STRING holding
//! JSON, `hydrate::tolerant_opt_json`), so a reader that trusted its shape
//! would panic or blank the whole face over one bad entry. Instead: a string
//! blob is re-parsed, an entry missing its `topic`, `label` or
//! `attachmentId` is DROPPED, a width/height that is not a positive whole
//! number reads as unknown, unknown fields are ignored, and the list is
//! capped at [`MAX_SESSION_RESULTS`] — the same cap the server enforces, so
//! a row that somehow exceeds it renders the first 60 rather than nothing.

use serde_json::Value;

/// The server's cap on one run's results — mirrored here so an over-long
/// blob truncates rather than renders unbounded.
pub const MAX_SESSION_RESULTS: usize = 60;

/// The Results face's tile HEIGHT in px. Every tile in a topic's row is this
/// tall and takes its width from the probed aspect
/// ([`session_result_tile_width`]), so an iOS, an Android and a web shot of
/// the same screen line up on one baseline instead of stair-stepping.
pub const SESSION_RESULT_TILE_HEIGHT: f32 = 320.0;

/// One published picture.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionResultEntry {
    /// The heading the agent filed it under (the group band's text).
    pub topic: String,
    /// The one-line caption under the tile (usually the platform).
    pub label: String,
    /// The attachment row — the image is `/api/attachments/{id}`
    /// (member-gated, exactly like a comment attachment).
    pub attachment_id: String,
    /// The server's probe, when it got one: positive whole pixels or
    /// `None` (a tile with either side unknown falls back to 4:3).
    pub width: Option<u32>,
    pub height: Option<u32>,
}

/// One topic's tiles, in the order they were published.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionResultGroup {
    pub topic: String,
    pub entries: Vec<SessionResultEntry>,
}

/// Read `coding_sessions.results` — see the module docs for the tolerance
/// rules. Anything that is not an array (or a JSON string holding one)
/// yields an empty list.
pub fn parse_session_results(raw: Option<&Value>) -> Vec<SessionResultEntry> {
    let Some(raw) = raw else {
        return Vec::new();
    };
    // The store hands a jsonb column over as TEXT; a wire-delivered array
    // arrives structured. Both are the same list.
    let reparsed;
    let items = match raw {
        Value::Array(items) => items,
        Value::String(text) => {
            reparsed = serde_json::from_str::<Value>(text).ok();
            match reparsed.as_ref() {
                Some(Value::Array(items)) => items,
                _ => return Vec::new(),
            }
        }
        _ => return Vec::new(),
    };
    items
        .iter()
        .filter_map(entry_from)
        .take(MAX_SESSION_RESULTS)
        .collect()
}

/// Group parsed entries by topic in FIRST-SEEN order — the page's reading
/// order is the agent's publishing order, never an alphabetical re-sort.
pub fn group_session_results(entries: &[SessionResultEntry]) -> Vec<SessionResultGroup> {
    let mut groups: Vec<SessionResultGroup> = Vec::new();
    for entry in entries {
        match groups.iter_mut().find(|group| group.topic == entry.topic) {
            Some(group) => group.entries.push(entry.clone()),
            None => groups.push(SessionResultGroup {
                topic: entry.topic.clone(),
                entries: vec![entry.clone()],
            }),
        }
    }
    groups
}

/// The tile's width at `height`: the probed aspect, or 4:3 when either side
/// is unknown. Rounded — a fractional pixel width leaves a hairline seam
/// between a tile and its border.
pub fn session_result_tile_width(entry: &SessionResultEntry, height: f32) -> f32 {
    let aspect = match (entry.width, entry.height) {
        (Some(width), Some(tall)) if width > 0 && tall > 0 => width as f32 / tall as f32,
        _ => 4.0 / 3.0,
    };
    (height * aspect).round()
}

/// One array element → an entry, or `None` when it is not usable.
fn entry_from(value: &Value) -> Option<SessionResultEntry> {
    let object = value.as_object()?;
    let text = |key: &str| -> Option<String> {
        let value = object.get(key)?.as_str()?.trim();
        (!value.is_empty()).then(|| value.to_string())
    };
    Some(SessionResultEntry {
        topic: text("topic")?,
        label: text("label")?,
        attachment_id: text("attachmentId")?,
        width: dimension(object.get("width")),
        height: dimension(object.get("height")),
    })
}

/// A probed pixel side: a POSITIVE WHOLE number, or unknown. A float with a
/// fraction, zero, a negative and a non-numeric all read as unknown rather
/// than as a bogus aspect.
fn dimension(value: Option<&Value>) -> Option<u32> {
    let value = value?;
    let number = match value {
        Value::Number(number) => number.as_f64()?,
        // Electric ships integer columns as text; a blob written through a
        // sloppy client can do the same inside the document.
        Value::String(text) => text.trim().parse::<f64>().ok()?,
        _ => return None,
    };
    if !number.is_finite() || number <= 0.0 || number.fract() != 0.0 {
        return None;
    }
    Some(number as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(topic: &str, label: &str, id: &str) -> SessionResultEntry {
        SessionResultEntry {
            topic: topic.to_string(),
            label: label.to_string(),
            attachment_id: id.to_string(),
            width: None,
            height: None,
        }
    }

    /// The ORDER of the blob is the order of the page, and one malformed
    /// entry costs that entry — never the whole face.
    #[test]
    fn parse_session_results_parses_a_flat_ordered_list_and_drops_malformed_entries() {
        let raw = serde_json::json!([
            {"topic": "Results page", "label": "Web", "attachmentId": "a1", "width": 1440, "height": 900},
            {"topic": "Results page", "label": "  ", "attachmentId": "a2"},
            {"topic": "", "label": "iOS", "attachmentId": "a3"},
            {"topic": "Results page", "label": "iOS", "attachmentId": ""},
            {"topic": "Results page", "label": "iOS"},
            "not an object",
            {"topic": "Results page", "label": "iOS", "attachmentId": "a4", "width": 0, "height": -3},
            {"topic": "Empty state", "label": "Android", "attachmentId": "a5", "width": 12.5, "height": 7},
        ]);
        let parsed = parse_session_results(Some(&raw));
        assert_eq!(
            parsed.iter().map(|e| e.attachment_id.as_str()).collect::<Vec<_>>(),
            vec!["a1", "a4", "a5"],
        );
        assert_eq!(parsed[0].topic, "Results page");
        assert_eq!(parsed[0].label, "Web");
        assert_eq!((parsed[0].width, parsed[0].height), (Some(1440), Some(900)));
        // Non-positive sides read as unknown, not as a bogus aspect.
        assert_eq!((parsed[1].width, parsed[1].height), (None, None));
        // A fractional side is not a pixel count.
        assert_eq!((parsed[2].width, parsed[2].height), (None, Some(7)));
    }

    /// A jsonb column hydrates out of SQLite as a STRING holding the array,
    /// unknown fields are simply ignored, and nothing usable reads as empty.
    #[test]
    fn parse_session_results_ignores_unknown_fields_and_a_null_or_blank_blob() {
        let as_string = serde_json::json!(
            r#"[{"topic":"Work header","label":"Desktop","attachmentId":"a1","kind":"screenshot","storageKey":"x"}]"#
        );
        let parsed = parse_session_results(Some(&as_string));
        assert_eq!(parsed, vec![entry("Work header", "Desktop", "a1")]);
        assert!(parse_session_results(None).is_empty());
        assert!(parse_session_results(Some(&Value::Null)).is_empty());
        assert!(parse_session_results(Some(&serde_json::json!("[]"))).is_empty());
        assert!(parse_session_results(Some(&serde_json::json!("not json"))).is_empty());
        assert!(parse_session_results(Some(&serde_json::json!({"topic": "x"}))).is_empty());
    }

    /// Topics band in the order they were first published — a later entry
    /// rejoins its topic rather than opening a second band for it.
    #[test]
    fn group_session_results_groups_by_topic_in_first_seen_order() {
        let entries = vec![
            entry("Run face", "Web", "a1"),
            entry("Results face", "Web", "a2"),
            entry("Run face", "iOS", "a3"),
            entry("Results face", "Android", "a4"),
        ];
        let groups = group_session_results(&entries);
        assert_eq!(
            groups.iter().map(|g| g.topic.as_str()).collect::<Vec<_>>(),
            vec!["Run face", "Results face"],
        );
        assert_eq!(
            groups[0].entries.iter().map(|e| e.attachment_id.as_str()).collect::<Vec<_>>(),
            vec!["a1", "a3"],
        );
        assert_eq!(
            groups[1].entries.iter().map(|e| e.attachment_id.as_str()).collect::<Vec<_>>(),
            vec!["a2", "a4"],
        );
        assert!(group_session_results(&[]).is_empty());
    }

    /// The server's cap, mirrored: an over-long blob renders its first 60,
    /// never nothing and never unbounded.
    #[test]
    fn parse_session_results_caps_at_60_entries() {
        let items: Vec<Value> = (0..80)
            .map(|index| {
                serde_json::json!({
                    "topic": "Shots",
                    "label": "Web",
                    "attachmentId": format!("a{index}"),
                })
            })
            .collect();
        let parsed = parse_session_results(Some(&Value::Array(items)));
        assert_eq!(parsed.len(), MAX_SESSION_RESULTS);
        assert_eq!(parsed[0].attachment_id, "a0");
        assert_eq!(parsed[MAX_SESSION_RESULTS - 1].attachment_id, "a59");
    }

    /// Equal-height tiles: the width follows the probed aspect, and a tile
    /// missing either side falls back to 4:3 rather than to a square.
    #[test]
    fn session_result_tile_width_sizes_a_tile_from_the_probed_aspect_4_3_without_one() {
        let mut shot = entry("Shots", "Web", "a1");
        shot.width = Some(1440);
        shot.height = Some(900);
        assert_eq!(
            session_result_tile_width(&shot, SESSION_RESULT_TILE_HEIGHT),
            512.0,
        );
        // A phone shot is TALLER than it is wide — the tile narrows.
        let mut phone = entry("Shots", "iOS", "a2");
        phone.width = Some(1170);
        phone.height = Some(2532);
        assert_eq!(session_result_tile_width(&phone, 320.0), 148.0);
        // Either side unknown = 4:3.
        let unknown = entry("Shots", "Android", "a3");
        assert_eq!(session_result_tile_width(&unknown, 300.0), 400.0);
        let mut half = entry("Shots", "Android", "a4");
        half.width = Some(800);
        assert_eq!(session_result_tile_width(&half, 300.0), 400.0);
    }
}
