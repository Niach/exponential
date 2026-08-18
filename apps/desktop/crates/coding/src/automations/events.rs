//! Event matching (EXP-530): synced `issue_events` rows vs an event
//! trigger's filters. Pure — hosts pre-fetch the candidate rows (≤ the
//! contract catch-up window) and pre-join `board_id` from issues (the
//! issue_events shape carries none). Filter posture is CONSERVATIVE: a
//! non-empty filter with a missing datum FAILS — an automation must never
//! fire on a row it could not check.

use super::trigger::{EventKind, EventSpec};

/// One pre-fetched issue event, host-shaped for the engine.
#[derive(Clone, Debug, PartialEq)]
pub struct EventRow {
    pub id: String,
    pub issue_id: String,
    /// `created_at` as a ms epoch (the watermark scale).
    pub created_at_ms: i64,
    /// The raw `issue_event_type` wire value.
    pub kind: String,
    pub payload: Option<serde_json::Value>,
    /// Pre-joined from the issue row; `None` = issue unknown locally.
    pub board_id: Option<String>,
}

/// Whether one row satisfies the spec's kind + every non-empty filter.
pub fn event_matches(spec: &EventSpec, row: &EventRow) -> bool {
    if row.kind != spec.event.wire() {
        return false;
    }
    if !passes(&spec.board_ids, row.board_id.as_deref()) {
        return false;
    }
    // Per-kind payload filters. The kinds without a payload filter
    // (assignee_changed, pr_opened, pr_merged) take only the board filter.
    match spec.event {
        EventKind::LabelAdded => passes(&spec.label_ids, payload_str(row, "labelId")),
        EventKind::StatusChanged => passes(&spec.to_status_ids, payload_str(row, "toStatusId")),
        EventKind::Created => passes(&spec.priorities, payload_str(row, "priority")),
        EventKind::PriorityChanged => passes(&spec.priorities, payload_str(row, "to")),
        EventKind::AssigneeChanged | EventKind::PrOpened | EventKind::PrMerged => true,
    }
}

/// The rows past the watermark, inside the catch-up window, matching the
/// spec — sorted by `(created_at_ms, id)` (the watermark's tuple order).
pub fn matching_events<'a>(
    spec: &EventSpec,
    rows: &'a [EventRow],
    watermark: (i64, &str),
    now_ms: i64,
) -> Vec<&'a EventRow> {
    let cutoff = now_ms - domain::contract::AUTOMATION_EVENT_CATCHUP_MS;
    let mut matches: Vec<&EventRow> = rows
        .iter()
        .filter(|row| (row.created_at_ms, row.id.as_str()) > watermark)
        .filter(|row| row.created_at_ms >= cutoff)
        .filter(|row| event_matches(spec, row))
        .collect();
    matches.sort_by(|a, b| (a.created_at_ms, a.id.as_str()).cmp(&(b.created_at_ms, b.id.as_str())));
    matches
}

/// Empty filter = absent (passes); non-empty requires a PRESENT, listed
/// datum.
fn passes(filter: &[String], datum: Option<&str>) -> bool {
    filter.is_empty() || datum.is_some_and(|value| filter.iter().any(|id| id == value))
}

fn payload_str<'a>(row: &'a EventRow, key: &str) -> Option<&'a str> {
    row.payload.as_ref()?.get(key)?.as_str()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::automations::trigger::EventKind;
    use serde_json::json;

    fn spec(event: EventKind) -> EventSpec {
        EventSpec {
            event,
            board_ids: Vec::new(),
            label_ids: Vec::new(),
            priorities: Vec::new(),
            to_status_ids: Vec::new(),
        }
    }

    fn row(id: &str, kind: &str, created_at_ms: i64) -> EventRow {
        EventRow {
            id: id.to_string(),
            issue_id: "issue-1".to_string(),
            created_at_ms,
            kind: kind.to_string(),
            payload: None,
            board_id: Some("board-1".to_string()),
        }
    }

    /// The filter matrix, one clause per event kind (truth-table idiom).
    #[test]
    fn filter_matrix() {
        // Kind must equal the spec's wire value.
        assert!(event_matches(&spec(EventKind::Created), &row("e", "created", 0)));
        assert!(!event_matches(&spec(EventKind::Created), &row("e", "status_changed", 0)));

        // Board filter: empty passes; listed board passes; unlisted or
        // MISSING board fails (conservative).
        let mut boarded = spec(EventKind::PrOpened);
        boarded.board_ids = vec!["board-1".to_string()];
        assert!(event_matches(&boarded, &row("e", "pr_opened", 0)));
        let mut other = row("e", "pr_opened", 0);
        other.board_id = Some("board-2".to_string());
        assert!(!event_matches(&boarded, &other));
        other.board_id = None;
        assert!(!event_matches(&boarded, &other), "missing datum fails a non-empty filter");

        // label_added ↔ payload.labelId.
        let mut labeled = spec(EventKind::LabelAdded);
        labeled.label_ids = vec!["lbl-1".to_string()];
        let mut label_row = row("e", "label_added", 0);
        label_row.payload = Some(json!({"labelId": "lbl-1"}));
        assert!(event_matches(&labeled, &label_row));
        label_row.payload = Some(json!({"labelId": "lbl-2"}));
        assert!(!event_matches(&labeled, &label_row));
        label_row.payload = None;
        assert!(!event_matches(&labeled, &label_row));

        // status_changed ↔ payload.toStatusId.
        let mut to_status = spec(EventKind::StatusChanged);
        to_status.to_status_ids = vec!["st-1".to_string()];
        let mut status_row = row("e", "status_changed", 0);
        status_row.payload = Some(json!({"toStatusId": "st-1", "fromStatusId": "st-0"}));
        assert!(event_matches(&to_status, &status_row));
        status_row.payload = Some(json!({"fromStatusId": "st-0"}));
        assert!(!event_matches(&to_status, &status_row));

        // priorities ↔ payload.priority (created) / payload.to
        // (priority_changed).
        let mut created = spec(EventKind::Created);
        created.priorities = vec!["urgent".to_string()];
        let mut created_row = row("e", "created", 0);
        created_row.payload = Some(json!({"priority": "urgent"}));
        assert!(event_matches(&created, &created_row));
        created_row.payload = Some(json!({"priority": "low"}));
        assert!(!event_matches(&created, &created_row));
        let mut changed = spec(EventKind::PriorityChanged);
        changed.priorities = vec!["urgent".to_string()];
        let mut changed_row = row("e", "priority_changed", 0);
        changed_row.payload = Some(json!({"from": "low", "to": "urgent"}));
        assert!(event_matches(&changed, &changed_row));
        changed_row.payload = Some(json!({"from": "urgent", "to": "low"}));
        assert!(!event_matches(&changed, &changed_row));

        // assignee_changed takes only the board filter — a stray priorities
        // list on the spec is ignored, not conservatively failed (the
        // server's zod forbids that shape; the parser keeps it empty).
        assert!(event_matches(&spec(EventKind::AssigneeChanged), &row("e", "assignee_changed", 0)));
        assert!(event_matches(&spec(EventKind::PrMerged), &row("e", "pr_merged", 0)));
    }

    #[test]
    fn watermark_is_tuple_ordered() {
        let rows = vec![
            row("id-a", "created", 100),
            row("id-b", "created", 100),
            row("id-c", "created", 200),
        ];
        let now = 1_000;
        // Equal created_at: only ids ABOVE the watermark id pass.
        let past_a = matching_events(&spec(EventKind::Created), &rows, (100, "id-a"), now);
        assert_eq!(
            past_a.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(),
            vec!["id-b", "id-c"]
        );
        // The watermark row itself is excluded (strictly greater).
        let past_c = matching_events(&spec(EventKind::Created), &rows, (200, "id-c"), now);
        assert!(past_c.is_empty());
        // A lower timestamp never passes regardless of id.
        let fresh = matching_events(&spec(EventKind::Created), &rows, (150, ""), now);
        assert_eq!(fresh.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(), vec!["id-c"]);
    }

    #[test]
    fn catchup_cutoff_drops_stale_rows() {
        let now = domain::contract::AUTOMATION_EVENT_CATCHUP_MS + 500;
        let rows = vec![
            row("old", "created", 499),  // just outside the window
            row("edge", "created", 500), // exactly at the cutoff — kept
            row("new", "created", now - 1),
        ];
        let matches = matching_events(&spec(EventKind::Created), &rows, (0, ""), now);
        assert_eq!(
            matches.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(),
            vec!["edge", "new"]
        );
    }

    #[test]
    fn matches_sort_by_created_at_then_id() {
        let rows = vec![
            row("id-z", "created", 300),
            row("id-b", "created", 100),
            row("id-a", "created", 300),
        ];
        let matches = matching_events(&spec(EventKind::Created), &rows, (0, ""), 1_000);
        assert_eq!(
            matches.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(),
            vec!["id-b", "id-a", "id-z"]
        );
    }
}
