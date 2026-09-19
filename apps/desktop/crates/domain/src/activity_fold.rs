//! EXP-900: activity folding — READ-TIME ONLY. `issue_events` rows are never
//! deleted or rewritten; every client folds the synced rows itself with this
//! pure function (mirrored: web `apps/web/src/lib/activity/fold.ts`, iOS
//! `ExpCore/ActivityFold.swift`, Android `domain/ActivityFold.kt`), all locked
//! against `packages/domain-contract/fixtures/activity-fold.json` — the SAME
//! case names everywhere. A "Show all" toggle on the timeline header returns
//! the unfolded list (EXP-468, [`crate`]'s consumer is `ui::timeline`).
//!
//! THE FOLD RULE: a run of events on the SAME issue and the SAME field by the
//! SAME actor, with no event (or comment, see [`ActivityBarrier`]) from any
//! OTHER actor on that issue in between, first-to-last within
//! [`FOLD_WINDOW_MS`], collapses to its NET effect. A net of "nothing changed"
//! (A → B → A) disappears entirely; a net A → C shows as ONE event carrying
//! the first event's `from*` payload keys and the last event's everything
//! else, timestamped at the LAST event and keyed by ITS id. `created`,
//! `pr_opened` and `pr_merged` never fold ([`fold_field_key`] → `None`) and
//! never break a run of the same actor; a run spanning MORE than the window
//! from first to last is left alone entirely (no partial fold). An event
//! without an actor never folds and breaks every open run on its issue.
//!
//! "Field" derives from the event type: `status_changed` → status,
//! `assignee_changed` → assignee, `priority_changed` → priority,
//! `board_moved` → board, `label_added`/`label_removed` → that ONE label
//! (`payload.labelId`; an add and a remove of the same label cancel),
//! `relation_added`/`relation_removed` → that one relation (`payload.type` +
//! `payload.relatedIssueId`).

use std::collections::HashMap;

use serde_json::{Map, Value};

use crate::rows::IssueEvent;

/// Something that is not an event but still breaks another actor's run: a
/// comment. "Reviewer says no" is usually a comment, not a status change, so
/// the timeline passes its comments here and the dev's progress → review →
/// progress round trip stays visible. Barriers are never returned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActivityBarrier {
    pub issue_id: String,
    pub actor_user_id: Option<String>,
    pub created_at: String,
}

/// First-to-last span a run may cover and still fold (10 minutes, in millis).
pub const FOLD_WINDOW_MS: i64 = 10 * 60 * 1000;

/// The payload object, or an empty one (web `payloadOf`: a missing or
/// non-object payload reads as `{}`).
fn payload_of(event: &IssueEvent) -> Map<String, Value> {
    match event.payload.as_ref() {
        Some(Value::Object(map)) => map.clone(),
        _ => Map::new(),
    }
}

/// Web `optionalString`: a non-empty STRING, anything else is `None`.
fn optional_string(value: Option<&Value>) -> Option<String> {
    match value {
        Some(Value::String(text)) if !text.is_empty() => Some(text.clone()),
        _ => None,
    }
}

/// Web `String(payload.a ?? payload.b ?? "")` — the first key that is present
/// and not null, stringified; `""` when none is.
fn coalesced_string(payload: &Map<String, Value>, keys: &[&str]) -> String {
    for key in keys {
        match payload.get(*key) {
            None | Some(Value::Null) => continue,
            Some(Value::String(text)) => return text.clone(),
            Some(other) => return other.to_string(),
        }
    }
    String::new()
}

fn kind_of(event: &IssueEvent) -> &str {
    event.kind.as_deref().unwrap_or_default()
}

/// The field an event edits, or `None` for an event that never folds.
pub fn fold_field_key(event: &IssueEvent) -> Option<String> {
    let payload = payload_of(event);
    match kind_of(event) {
        "status_changed" => Some("status".to_string()),
        "assignee_changed" => Some("assignee".to_string()),
        "priority_changed" => Some("priority".to_string()),
        "board_moved" => Some("board".to_string()),
        "label_added" | "label_removed" => {
            optional_string(payload.get("labelId")).map(|label_id| format!("label:{label_id}"))
        }
        "relation_added" | "relation_removed" => {
            let kind = optional_string(payload.get("type"))?;
            let related = optional_string(payload.get("relatedIssueId"))?;
            Some(format!("relation:{kind}:{related}"))
        }
        _ => None,
    }
}

// The "before" side of the first event and the "after" side of the last one,
// as comparable strings. Equal = the run changed nothing. A status compares
// on the precise `statusId` pair when the payload carries one (EXP-314) and
// on the legacy enum otherwise; presence toggles (labels, relations) compare
// as present/absent.
fn before_of(event: &IssueEvent) -> String {
    let payload = payload_of(event);
    match kind_of(event) {
        "status_changed" => coalesced_string(&payload, &["fromStatusId", "from"]),
        "assignee_changed" | "priority_changed" => coalesced_string(&payload, &["from"]),
        "board_moved" => coalesced_string(&payload, &["fromBoardId"]),
        "label_added" | "relation_added" => "absent".to_string(),
        "label_removed" | "relation_removed" => "present".to_string(),
        _ => String::new(),
    }
}

fn after_of(event: &IssueEvent) -> String {
    let payload = payload_of(event);
    match kind_of(event) {
        "status_changed" => coalesced_string(&payload, &["toStatusId", "to"]),
        "assignee_changed" | "priority_changed" => coalesced_string(&payload, &["to"]),
        "board_moved" => coalesced_string(&payload, &["toBoardId"]),
        "label_added" | "relation_added" => "present".to_string(),
        "label_removed" | "relation_removed" => "absent".to_string(),
        _ => String::new(),
    }
}

/// The last event wearing the first event's `from*` keys.
fn merge_payload(first: &IssueEvent, last: &IssueEvent) -> Value {
    let mut merged = Map::new();
    for (key, value) in payload_of(last) {
        if !key.starts_with("from") {
            merged.insert(key, value);
        }
    }
    for (key, value) in payload_of(first) {
        if key.starts_with("from") {
            merged.insert(key, value);
        }
    }
    Value::Object(merged)
}

/// One event on its way out: the row, its input index and the instant it
/// sorts on (see [`sort_instant`]).
struct Out {
    event: IssueEvent,
    index: i64,
    at: i64,
}

struct Run {
    issue_id: String,
    actor_user_id: String,
    events: Vec<Out>,
}

fn flush_run(run: Run, out: &mut Vec<Out>) {
    let mut events = run.events;
    if events.len() <= 1 {
        out.append(&mut events);
        return;
    }
    let last_index = events.len() - 1;
    // Both ends parsed — an unparseable event never joins a run.
    if events[last_index].at - events[0].at > FOLD_WINDOW_MS {
        out.append(&mut events);
        return;
    }
    if before_of(&events[0].event) == after_of(&events[last_index].event) {
        return;
    }
    let payload = merge_payload(&events[0].event, &events[last_index].event);
    let mut folded = events.pop().expect("the run holds at least two events");
    folded.event.payload = Some(payload);
    out.push(folded);
}

/// Events (chronological, oldest first) folded to their net effect; output
/// chronological too. `barriers` (comments) only ever break runs, they are
/// never returned. Inputs are never mutated.
pub fn fold_activity(events: &[IssueEvent], barriers: &[ActivityBarrier]) -> Vec<IssueEvent> {
    #[derive(Clone, Copy)]
    enum Step<'a> {
        Event {
            event: &'a IssueEvent,
            /// `None` = an unparseable `created_at`: never folds.
            instant: Option<i64>,
        },
        Barrier(&'a ActivityBarrier),
    }

    // (sort instant, sort index, step). A barrier at the same instant as an
    // event sorts BEFORE it (web: `index - barriers.length`), so a comment
    // written together with a change still separates it from what follows.
    let mut steps: Vec<(i64, i64, Step<'_>)> = Vec::with_capacity(events.len() + barriers.len());
    let mut carried = i64::MIN;
    for (index, event) in events.iter().enumerate() {
        let instant = event.created_at.as_deref().and_then(parse_instant);
        let at = sort_instant(instant, &mut carried);
        steps.push((at, index as i64, Step::Event { event, instant }));
    }
    let mut carried = i64::MIN;
    for (index, barrier) in barriers.iter().enumerate() {
        let at = sort_instant(parse_instant(&barrier.created_at), &mut carried);
        steps.push((at, index as i64 - barriers.len() as i64, Step::Barrier(barrier)));
    }
    steps.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));

    // Open runs keyed by `{issue_id} {actor} {field}`, in insertion order so
    // a flush stays deterministic.
    let mut open: HashMap<String, Run> = HashMap::new();
    let mut open_keys: Vec<String> = Vec::new();
    let mut out: Vec<Out> = Vec::new();

    for (at, index, step) in steps {
        let (issue_id, actor): (&str, Option<&str>) = match step {
            Step::Event { event, .. } => (event.issue_id.as_str(), event.actor_user_id.as_deref()),
            Step::Barrier(barrier) => (barrier.issue_id.as_str(), barrier.actor_user_id.as_deref()),
        };
        // Everything by ANOTHER actor (or by nobody) on this issue flushes
        // every open run there.
        open_keys.retain(|key| {
            let Some(run) = open.get(key) else {
                return false;
            };
            if run.issue_id != issue_id {
                return true;
            }
            if actor.is_some_and(|actor| run.actor_user_id == actor) {
                return true;
            }
            if let Some(run) = open.remove(key) {
                flush_run(run, &mut out);
            }
            false
        });

        let Step::Event { event, instant } = step else {
            continue;
        };
        let field = match (actor, instant) {
            // An event with no actor, no foldable field, or an unparseable
            // timestamp passes through in place.
            (Some(_), Some(_)) => fold_field_key(event),
            _ => None,
        };
        let (Some(actor), Some(field)) = (actor, field) else {
            out.push(Out {
                event: event.clone(),
                index,
                at,
            });
            continue;
        };
        let entry = Out {
            event: event.clone(),
            index,
            at,
        };
        let key = format!("{} {} {}", event.issue_id, actor, field);
        match open.get_mut(&key) {
            Some(run) => run.events.push(entry),
            None => {
                open.insert(
                    key.clone(),
                    Run {
                        issue_id: event.issue_id.clone(),
                        actor_user_id: actor.to_string(),
                        events: vec![entry],
                    },
                );
                open_keys.push(key);
            }
        }
    }
    for key in open_keys {
        if let Some(run) = open.remove(&key) {
            flush_run(run, &mut out);
        }
    }

    out.sort_by(|a, b| a.at.cmp(&b.at).then_with(|| a.index.cmp(&b.index)));
    out.into_iter().map(|entry| entry.event).collect()
}

/// The instant a step SORTS on. The web comparator (`a.at - b.at ||
/// a.index - b.index`) degenerates to the index compare when an instant is
/// `NaN`; Rust needs a total order, so an unparseable timestamp inherits the
/// previous input row's instant and the index tiebreak keeps it in place.
fn sort_instant(instant: Option<i64>, carried: &mut i64) -> i64 {
    match instant {
        Some(at) => {
            *carried = at;
            at
        }
        None => *carried,
    }
}

/// `YYYY-MM-DD[(T| )hh:mm[:ss[.fff]][Z|±hh[:mm]]]` → epoch millis (the web's
/// `Date.parse`). Covers both ISO-8601 (`…T…Z`) and Electric's timestamptz
/// text (`… …+00`), the two forms a row's timestamps ever arrive in. Twin of
/// `issue_search::parse_instant` (private there; this crate keeps every rule
/// module self-contained).
fn parse_instant(value: &str) -> Option<i64> {
    let value = value.trim();
    let year: i64 = value.get(0..4)?.parse().ok()?;
    if value.get(4..5)? != "-" {
        return None;
    }
    let month: i64 = value.get(5..7)?.parse().ok()?;
    if value.get(7..8)? != "-" {
        return None;
    }
    let day: i64 = value.get(8..10)?.parse().ok()?;
    let mut millis = days_from_civil(year, month, day) * 86_400_000;

    let Some(rest) = value.get(10..).filter(|rest| !rest.is_empty()) else {
        return Some(millis);
    };
    if !matches!(rest.get(0..1)?, "T" | "t" | " ") {
        return None;
    }
    let rest = rest.get(1..)?;
    let hour: i64 = rest.get(0..2)?.parse().ok()?;
    if rest.get(2..3)? != ":" {
        return None;
    }
    let minute: i64 = rest.get(3..5)?.parse().ok()?;
    millis += hour * 3_600_000 + minute * 60_000;
    let mut rest = rest.get(5..)?;

    if rest.starts_with(':') {
        let second: i64 = rest.get(1..3)?.parse().ok()?;
        millis += second * 1_000;
        rest = rest.get(3..)?;
        if let Some(fraction) = rest.strip_prefix('.') {
            let digits: String = fraction.chars().take_while(char::is_ascii_digit).collect();
            if digits.is_empty() {
                return None;
            }
            let mut frac = digits.clone();
            frac.truncate(3);
            while frac.len() < 3 {
                frac.push('0');
            }
            millis += frac.parse::<i64>().ok()?;
            rest = &rest[1 + digits.len()..];
        }
    }

    if rest.is_empty() || rest.eq_ignore_ascii_case("z") {
        return Some(millis);
    }
    // An offset moves the instant the OTHER way (`+02:00` = 2h earlier UTC).
    let sign: i64 = match rest.get(0..1)? {
        "+" => -1,
        "-" => 1,
        _ => return None,
    };
    let rest = rest.get(1..)?;
    let offset_hours: i64 = rest.get(0..2)?.parse().ok()?;
    let offset_minutes: i64 = match rest.get(2..3) {
        Some(":") => rest.get(3..5).and_then(|m| m.parse().ok()).unwrap_or(0),
        Some(_) => rest.get(2..4).and_then(|m| m.parse().ok()).unwrap_or(0),
        None => 0,
    };
    Some(millis + sign * (offset_hours * 3_600_000 + offset_minutes * 60_000))
}

/// Days since the Unix epoch for a civil date (Howard Hinnant's `days_from_civil`).
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let year_of_era = year - era * 400;
    let day_of_year = (153 * (if month > 2 { month - 3 } else { month + 9 }) + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    /// The ONE contract fixture, replayed byte-for-byte on every client (web
    /// `fold.test.ts`, iOS `ActivityFoldTests`, Android `ActivityFoldTest`) —
    /// the SAME case names everywhere.
    const FIXTURE: &str =
        include_str!("../../../../../packages/domain-contract/fixtures/activity-fold.json");

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct FixtureEvent {
        id: String,
        #[serde(default)]
        issue_id: String,
        #[serde(default)]
        actor_user_id: Option<String>,
        #[serde(rename = "type")]
        kind: String,
        #[serde(default)]
        payload: Option<Value>,
        created_at: String,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct FixtureBarrier {
        issue_id: String,
        #[serde(default)]
        actor_user_id: Option<String>,
        created_at: String,
    }

    #[derive(Deserialize)]
    struct FixtureCase {
        name: String,
        events: Vec<FixtureEvent>,
        #[serde(default)]
        barriers: Vec<FixtureBarrier>,
        expected: Vec<FixtureEvent>,
    }

    impl FixtureEvent {
        fn row(&self) -> IssueEvent {
            IssueEvent {
                id: self.id.clone(),
                issue_id: self.issue_id.clone(),
                team_id: None,
                actor_user_id: self.actor_user_id.clone(),
                kind: Some(self.kind.clone()),
                payload: self.payload.clone(),
                created_at: Some(self.created_at.clone()),
                updated_at: None,
            }
        }
    }

    /// `(id, type, payload, created_at)` — the payload as a PARSED value, never
    /// as object text (key order differs between crates).
    fn shape(event: &IssueEvent) -> (String, Option<String>, Value, Option<String>) {
        (
            event.id.clone(),
            event.kind.clone(),
            event.payload.clone().unwrap_or(Value::Null),
            event.created_at.clone(),
        )
    }

    /// Every fixture case, replayed with its own name in the failure message
    /// (a Rust test name cannot be a sentence — the fixture's name IS the case
    /// identity across the four clients).
    #[test]
    fn activity_fold_contract_fixture() {
        let cases: Vec<FixtureCase> =
            serde_json::from_str(FIXTURE).expect("the activity-fold fixture parses");
        assert!(!cases.is_empty());
        for case in &cases {
            let events: Vec<IssueEvent> = case.events.iter().map(FixtureEvent::row).collect();
            let barriers: Vec<ActivityBarrier> = case
                .barriers
                .iter()
                .map(|barrier| ActivityBarrier {
                    issue_id: barrier.issue_id.clone(),
                    actor_user_id: barrier.actor_user_id.clone(),
                    created_at: barrier.created_at.clone(),
                })
                .collect();
            let folded = fold_activity(&events, &barriers);
            let actual: Vec<_> = folded.iter().map(shape).collect();
            let expected: Vec<_> = case
                .expected
                .iter()
                .map(|event| event.row())
                .map(|event| shape(&event))
                .collect();
            assert_eq!(actual, expected, "case: {}", case.name);
        }
    }

    fn event(id: &str, actor: Option<&str>, kind: &str, payload: Value, at: &str) -> IssueEvent {
        IssueEvent {
            id: id.to_string(),
            issue_id: "i1".to_string(),
            team_id: None,
            actor_user_id: actor.map(str::to_string),
            kind: Some(kind.to_string()),
            payload: Some(payload),
            created_at: Some(at.to_string()),
            updated_at: None,
        }
    }

    fn status(id: &str, from: &str, to: &str, at: &str) -> IssueEvent {
        event(
            id,
            Some("A"),
            "status_changed",
            serde_json::json!({ "fromStatusId": from, "toStatusId": to }),
            at,
        )
    }

    #[test]
    fn folds_exactly_at_the_window_boundary_and_not_past_it() {
        let inside = vec![
            status("e1", "a", "b", "2026-09-19T10:00:00Z"),
            status("e2", "b", "c", "2026-09-19T10:10:00Z"),
        ];
        let folded = fold_activity(&inside, &[]);
        assert_eq!(folded.len(), 1, "10 minutes exactly still folds");
        assert_eq!(folded[0].id, "e2");

        let outside = vec![
            status("e1", "a", "b", "2026-09-19T10:00:00Z"),
            status("e2", "b", "c", "2026-09-19T10:10:00.001Z"),
        ];
        assert_eq!(fold_activity(&outside, &[]).len(), 2, "a millisecond past the window is left alone");
    }

    #[test]
    fn parses_the_postgres_space_form_too() {
        let events = vec![
            status("e1", "a", "b", "2026-09-19 10:00:00+00"),
            status("e2", "b", "c", "2026-09-19 10:01:00+00"),
        ];
        let folded = fold_activity(&events, &[]);
        assert_eq!(folded.len(), 1);
        assert_eq!(
            folded[0].payload,
            Some(serde_json::json!({ "fromStatusId": "a", "toStatusId": "c" }))
        );
    }

    #[test]
    fn an_unparseable_timestamp_never_folds() {
        let events = vec![
            status("e1", "a", "b", "not a date"),
            status("e2", "b", "a", "also not a date"),
        ];
        let folded = fold_activity(&events, &[]);
        let ids: Vec<&str> = folded.iter().map(|event| event.id.as_str()).collect();
        assert_eq!(ids, vec!["e1", "e2"]);
    }

    #[test]
    fn never_mutates_its_inputs() {
        let events = vec![
            status("e1", "a", "b", "2026-09-19T10:00:00Z"),
            status("e2", "b", "c", "2026-09-19T10:01:00Z"),
        ];
        let before = events.clone();
        let _ = fold_activity(&events, &[]);
        assert_eq!(events, before);
    }

    #[test]
    fn fold_field_key_names_the_edited_field() {
        assert_eq!(
            fold_field_key(&status("e1", "a", "b", "2026-09-19T10:00:00Z")).as_deref(),
            Some("status")
        );
        assert_eq!(
            fold_field_key(&event(
                "e2",
                Some("A"),
                "label_added",
                serde_json::json!({ "labelId": "L" }),
                "2026-09-19T10:00:00Z"
            ))
            .as_deref(),
            Some("label:L")
        );
        assert_eq!(
            fold_field_key(&event(
                "e3",
                Some("A"),
                "relation_added",
                serde_json::json!({ "type": "blocks", "relatedIssueId": "i2" }),
                "2026-09-19T10:00:00Z"
            ))
            .as_deref(),
            Some("relation:blocks:i2")
        );
        // A label row without an id, and the never-folding kinds.
        for never in ["label_added", "created", "pr_opened", "pr_merged"] {
            assert_eq!(
                fold_field_key(&event(
                    "e4",
                    Some("A"),
                    never,
                    serde_json::json!({}),
                    "2026-09-19T10:00:00Z"
                )),
                None,
                "{never} must not fold"
            );
        }
    }
}
