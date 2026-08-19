//! Event matching (EXP-530): synced `issue_events` rows vs an event
//! trigger's filters. Pure — hosts pre-fetch the candidate rows (≤ the
//! contract catch-up window) and pre-join `board_id` from issues (the
//! issue_events shape carries none). Filter posture is CONSERVATIVE: a
//! non-empty filter with a missing datum FAILS — an automation must never
//! fire on a row it could not check.

use std::collections::HashSet;

use super::trigger::{EventKind, EventSpec};

/// How far below the snapshot's newest row the [`EventCursor::Seen`] floor
/// trails (EXP-562). Two overlapping transactions can commit out of
/// `created_at` order and sync in that same wrong order; the earlier row
/// then lands BELOW a bare high-water mark and is lost forever. The grace
/// keeps re-considering that window and dedupes by id instead.
pub const EVENT_GRACE_MS: i64 = 60_000;

/// Cap on the ids a state remembers inside the grace window — a busy team
/// must not grow settings.json without bound. Overflow RAISES the floor
/// (see [`seen_window`]), which only ever narrows the grace.
pub const SEEN_IDS_CAP: usize = 128;

/// How a firing pass decides which rows it has already acted on.
#[derive(Clone, Copy, Debug)]
pub enum EventCursor<'a> {
    /// LEGACY (pre-EXP-562) states: strict `(created_at, id) >` tuple
    /// compare against the persisted high-water mark. Kept so an upgrade
    /// re-reads nothing it already fired on; the first fire migrates the
    /// state to [`EventCursor::Seen`].
    Strict { watermark: (i64, &'a str) },
    /// The grace cursor: everything at or above `floor` that is not
    /// already in `seen_ids`.
    Seen { floor: i64, seen_ids: &'a [String] },
}

/// One pre-fetched issue event, host-shaped for the engine.
#[derive(Clone, Debug, PartialEq)]
pub struct EventRow {
    pub id: String,
    pub issue_id: String,
    /// The event's denormalized `team_id` — a trigger only ever fires on
    /// its own action's team; `None` = unknown, which FAILS the team check.
    pub team_id: Option<String>,
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

/// The rows the `cursor` still admits, inside the catch-up window, on the
/// action's OWN team, matching the spec — sorted by `(created_at_ms, id)`.
/// The hosts sync every member team's events into one collection, so the
/// team fence lives here, not in the snapshot.
pub fn matching_events<'a>(
    spec: &EventSpec,
    team_id: &str,
    rows: &'a [EventRow],
    cursor: EventCursor<'_>,
    now_ms: i64,
) -> Vec<&'a EventRow> {
    let cutoff = now_ms - domain::contract::AUTOMATION_EVENT_CATCHUP_MS;
    // Built once per call — the seen set is O(cap) and the row scan O(n).
    let seen: HashSet<&str> = match cursor {
        EventCursor::Seen { seen_ids, .. } => seen_ids.iter().map(String::as_str).collect(),
        EventCursor::Strict { .. } => HashSet::new(),
    };
    let mut matches: Vec<&EventRow> = rows
        .iter()
        .filter(|row| row.team_id.as_deref() == Some(team_id))
        .filter(|row| match cursor {
            EventCursor::Strict { watermark } => (row.created_at_ms, row.id.as_str()) > watermark,
            EventCursor::Seen { floor, .. } => {
                row.created_at_ms >= floor && !seen.contains(row.id.as_str())
            }
        })
        .filter(|row| row.created_at_ms >= cutoff)
        .filter(|row| event_matches(spec, row))
        .collect();
    matches.sort_by(|a, b| (a.created_at_ms, a.id.as_str()).cmp(&(b.created_at_ms, b.id.as_str())));
    matches
}

/// The `(floor, seen_ids)` to persist after a seed or a fire: every MATCHING
/// same-team row at or above `base_floor`, newest first, truncated to
/// [`SEEN_IDS_CAP`].
///
/// Matching-only, deliberately, on both counts:
/// - It is SAFE. The seen set only has to cover rows this very spec would
///   fire on, and a trigger edit moves its fingerprint, which reseeds the
///   whole state rather than reusing these ids against a different spec.
/// - It is BETTER than remembering all rows. A row whose `board_id` join
///   came back `None` (its issue had not synced yet) does not match, so it
///   is NOT recorded as seen — when the issue lands, the row matches and
///   fires, as long as it is still inside the grace. Recording every row
///   would swallow it for good.
///
/// Overflow raises the floor past the oldest kept row (`+1`, so same-ms
/// ties go with it) and drops everything below: the window only ever
/// NARROWS, so truncation can lose a late row but never double-fire one.
pub fn seen_window(
    spec: &EventSpec,
    team_id: &str,
    rows: &[EventRow],
    base_floor: i64,
) -> (i64, Vec<String>) {
    let mut recent: Vec<&EventRow> = rows
        .iter()
        .filter(|row| row.team_id.as_deref() == Some(team_id))
        .filter(|row| row.created_at_ms >= base_floor)
        .filter(|row| event_matches(spec, row))
        .collect();
    recent.sort_by(|a, b| (b.created_at_ms, b.id.as_str()).cmp(&(a.created_at_ms, a.id.as_str())));
    let mut floor = base_floor;
    if recent.len() > SEEN_IDS_CAP {
        floor = recent[SEEN_IDS_CAP].created_at_ms + 1;
        recent.retain(|row| row.created_at_ms >= floor);
    }
    (floor, recent.into_iter().map(|row| row.id.clone()).collect())
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
            team_id: Some("team-1".to_string()),
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

    /// The hosts feed EVERY member team's events into one snapshot — a
    /// trigger must only ever see its own action's team, and a row with an
    /// unknown team fails conservatively.
    #[test]
    fn other_teams_rows_never_match() {
        let mut foreign = row("id-f", "created", 100);
        foreign.team_id = Some("team-2".to_string());
        let mut unknown = row("id-u", "created", 100);
        unknown.team_id = None;
        let rows = vec![row("id-m", "created", 100), foreign, unknown];
        let cursor = EventCursor::Strict { watermark: (0, "") };
        let matches = matching_events(&spec(EventKind::Created), "team-1", &rows, cursor, 1_000);
        assert_eq!(
            matches.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(),
            vec!["id-m"],
            "cross-team and team-less rows are fenced out"
        );
    }

    /// The legacy cursor (states written before EXP-562).
    #[test]
    fn watermark_is_tuple_ordered() {
        let rows = vec![
            row("id-a", "created", 100),
            row("id-b", "created", 100),
            row("id-c", "created", 200),
        ];
        let now = 1_000;
        let strict = |watermark| {
            let cursor = EventCursor::Strict { watermark };
            matching_events(&spec(EventKind::Created), "team-1", &rows, cursor, now)
        };
        // Equal created_at: only ids ABOVE the watermark id pass.
        let past_a = strict((100, "id-a"));
        assert_eq!(
            past_a.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(),
            vec!["id-b", "id-c"]
        );
        // The watermark row itself is excluded (strictly greater).
        assert!(strict((200, "id-c")).is_empty());
        // A lower timestamp never passes regardless of id.
        let fresh = strict((150, ""));
        assert_eq!(fresh.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(), vec!["id-c"]);
    }

    /// EXP-562: the grace cursor is floor-INCLUSIVE and id-deduped, so a
    /// row that committed early but synced late still fires exactly once.
    #[test]
    fn seen_cursor_admits_unseen_rows_at_or_above_floor() {
        let rows = vec![
            row("id-below", "created", 99),
            row("id-floor", "created", 100),
            row("id-seen", "created", 150),
            row("id-new", "created", 200),
        ];
        let seen = vec!["id-seen".to_string()];
        let cursor = EventCursor::Seen { floor: 100, seen_ids: &seen };
        let matches = matching_events(&spec(EventKind::Created), "team-1", &rows, cursor, 1_000);
        assert_eq!(
            matches.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(),
            vec!["id-floor", "id-new"],
            "at-the-floor passes, below-the-floor and already-seen do not"
        );
    }

    #[test]
    fn seen_window_keeps_matching_same_team_rows_only() {
        let mut foreign = row("id-foreign", "created", 200);
        foreign.team_id = Some("team-2".to_string());
        let mut unjoined = row("id-unjoined", "created", 200);
        unjoined.board_id = None;
        let mut boarded = spec(EventKind::Created);
        boarded.board_ids = vec!["board-1".to_string()];
        let rows = vec![
            row("id-old", "created", 99), // below the base floor
            row("id-a", "created", 100),
            row("id-b", "created", 300),
            row("id-other-kind", "status_changed", 300),
            foreign,
            unjoined,
        ];
        let (floor, ids) = seen_window(&boarded, "team-1", &rows, 100);
        assert_eq!(floor, 100, "under the cap the base floor stands");
        assert_eq!(
            ids,
            vec!["id-b".to_string(), "id-a".to_string()],
            "newest first; other teams, other kinds and sub-floor rows are out — and the \
             un-joined row is deliberately NOT seen, so it still fires once its issue syncs"
        );
    }

    #[test]
    fn seen_window_cap_raises_floor_past_dropped_rows() {
        // 3 rows share the oldest ms so the truncation has ties to prune.
        let mut rows: Vec<EventRow> = (0..SEEN_IDS_CAP as i64)
            .map(|index| row(&format!("id-new-{index:03}"), "created", 1_000 + index))
            .collect();
        for tie in 0..3 {
            rows.push(row(&format!("id-tie-{tie}"), "created", 500));
        }
        let (floor, ids) = seen_window(&spec(EventKind::Created), "team-1", &rows, 0);
        assert_eq!(floor, 501, "the floor lifts one ms past the oldest kept row");
        assert_eq!(ids.len(), SEEN_IDS_CAP, "the tied stragglers are all pruned");
        assert!(
            !ids.iter().any(|id| id.starts_with("id-tie-")),
            "same-ms ties go together — a kept-but-forgotten tie would re-fire"
        );
    }

    #[test]
    fn catchup_cutoff_drops_stale_rows() {
        let now = domain::contract::AUTOMATION_EVENT_CATCHUP_MS + 500;
        let rows = vec![
            row("old", "created", 499),  // just outside the window
            row("edge", "created", 500), // exactly at the cutoff — kept
            row("new", "created", now - 1),
        ];
        let cursor = EventCursor::Strict { watermark: (0, "") };
        let matches = matching_events(&spec(EventKind::Created), "team-1", &rows, cursor, now);
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
        let cursor = EventCursor::Strict { watermark: (0, "") };
        let matches = matching_events(&spec(EventKind::Created), "team-1", &rows, cursor, 1_000);
        assert_eq!(
            matches.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(),
            vec!["id-b", "id-a", "id-z"]
        );
    }
}
