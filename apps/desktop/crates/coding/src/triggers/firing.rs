//! The firing message (EXP-1292): what a trigger posts into its agent's
//! channel (`sessions.send({sessionId: agentId, text})`). It replaces the
//! old action prompt's `## Trigger` section (`TriggerNote`) and the two
//! host-side event-line renderers (desktop GUI + CLI daemon) with ONE pure
//! function: a schedule says which schedule fired, an event lists the
//! changed issues one line each (capped at [`FIRING_MAX_LINES`], the
//! overflow as "…and N more.").
//!
//! Hosts fill [`EventLookups`] from their own synced stores (the issues
//! named by [`firing_issue_ids`], plus the team-wide labels and statuses);
//! tests pass literals.

use std::collections::HashMap;

use super::events::EventRow;
use super::summary::schedule_phrase;
use super::trigger::TriggerKind;
use super::{ArmedTrigger, Firing};

/// Cap on the issue lines an event firing renders — the overflow becomes
/// one "…and N more." line.
pub const FIRING_MAX_LINES: usize = 50;

/// The names an event line renders (host-filled from the sync store).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct EventLookups {
    /// issue id → (identifier, title)
    pub issues: HashMap<String, (String, String)>,
    /// `issue_statuses` id → name
    pub statuses: HashMap<String, String>,
    /// label id → name
    pub labels: HashMap<String, String>,
}

/// The distinct issue ids an event firing's lines name (capped like the
/// lines themselves, first-seen order) — what a host point-reads into
/// [`EventLookups::issues`]. Empty for a schedule.
pub fn firing_issue_ids(firing: &Firing) -> Vec<String> {
    let Firing::Event { matches } = firing else {
        return Vec::new();
    };
    let mut ids: Vec<String> = Vec::new();
    for row in matches.iter().take(FIRING_MAX_LINES) {
        if !ids.contains(&row.issue_id) {
            ids.push(row.issue_id.clone());
        }
    }
    ids
}

/// The message a firing posts into its agent. Always within the server's
/// `sessions.send` text cap: lines that would overflow it fold into the
/// "…and N more." tail.
pub fn firing_text(armed: &ArmedTrigger, firing: &Firing, lookups: &EventLookups) -> String {
    match firing {
        Firing::Schedule { .. } => match &armed.trigger.kind {
            TriggerKind::Schedule(schedule) => format!(
                "Trigger fired: your schedule ({}, device time).",
                schedule_phrase(schedule)
            ),
            // Unreachable (the engine only schedule-fires a schedule).
            _ => "Trigger fired: your schedule.".to_string(),
        },
        Firing::Event { matches } => {
            let mut lines: Vec<String> = matches
                .iter()
                .take(FIRING_MAX_LINES)
                .filter_map(|row| event_line(row, lookups))
                .collect();
            if lines.is_empty() {
                // Every match names an issue this device has not synced: the
                // cursor already advanced past them, so still say it fired.
                let count = matches.len();
                let noun = if count == 1 { "change" } else { "changes" };
                return format!(
                    "Trigger fired: {count} issue {noun} matched, on issues this device has not \
synced."
                );
            }
            loop {
                let text = render_event(&lines, matches.len() - lines.len());
                let fits = text.encode_utf16().count() <= domain::contract::SESSIONS_SEND_TEXT_MAX;
                if fits || lines.len() == 1 {
                    return text;
                }
                lines.pop();
            }
        }
    }
}

fn render_event(lines: &[String], omitted: usize) -> String {
    let mut text = String::from("Trigger fired: these issues changed:\n");
    for line in lines {
        text.push_str(&format!("\n- {line}"));
    }
    if omitted > 0 {
        text.push_str(&format!("\n- …and {omitted} more."));
    }
    text
}

/// One line for a matched event: `EXP-42 "Title" <what changed>`. `None`
/// when the issue is not synced locally — a line naming no issue tells the
/// agent nothing (it counts into "…and N more" instead).
pub fn event_line(row: &EventRow, lookups: &EventLookups) -> Option<String> {
    let (identifier, title) = lookups.issues.get(&row.issue_id)?;
    let payload = |key: &str| {
        row.payload
            .as_ref()
            .and_then(|payload| payload.get(key))
            .and_then(serde_json::Value::as_str)
            .map(str::to_string)
    };
    let tail = match row.kind.as_str() {
        "status_changed" => format!(
            "status {} → {}",
            status_name(lookups, payload("fromName"), payload("fromStatusId"), payload("from")),
            status_name(lookups, payload("toName"), payload("toStatusId"), payload("to")),
        ),
        "priority_changed" => format!(
            "priority {} → {}",
            or_none(payload("from")),
            or_none(payload("to"))
        ),
        "created" => format!("created ({})", or_none(payload("priority"))),
        "label_added" => {
            let id = payload("labelId").unwrap_or_default();
            let name = lookups
                .labels
                .get(&id)
                .cloned()
                .filter(|name| !name.is_empty())
                .unwrap_or_else(|| if id.is_empty() { "a label".to_string() } else { id });
            format!("label {name} added")
        }
        "assignee_changed" => "assignee changed".to_string(),
        "pr_opened" => "pull request opened".to_string(),
        "pr_merged" => "pull request merged".to_string(),
        // Only the 7 contract kinds can match — a future one still reads.
        other => other.replace('_', " "),
    };
    Some(single_line(&format!("{identifier} \"{title}\" {tail}")))
}

/// A status side's display name: the payload's own snapshot (EXP-314 writes
/// `fromName`/`toName`), else the team's synced row, else the anchor enum
/// munged (`in_progress` → `in progress`). The chain never fails.
fn status_name(
    lookups: &EventLookups,
    name: Option<String>,
    status_id: Option<String>,
    anchor: Option<String>,
) -> String {
    if let Some(name) = name.filter(|name| !name.is_empty()) {
        return name;
    }
    if let Some(name) = status_id
        .and_then(|id| lookups.statuses.get(&id).cloned())
        .filter(|name| !name.is_empty())
    {
        return name;
    }
    match anchor.filter(|anchor| !anchor.is_empty()) {
        Some(anchor) => anchor.replace('_', " "),
        None => "none".to_string(),
    }
}

/// A cleared/absent priority reads `none` (the web `priorityLabel` shape).
fn or_none(value: Option<String>) -> String {
    value
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "none".to_string())
}

/// One list item per event: a title's newlines must not break the list,
/// and the server refuses NUL bytes in a message.
fn single_line(text: &str) -> String {
    text.replace('\0', "")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::triggers::trigger::{EventKind, EventSpec, ParsedTrigger, Schedule, ScheduleInterval};
    use serde_json::json;

    fn lookups() -> EventLookups {
        EventLookups {
            issues: [(
                "issue-1".to_string(),
                ("EXP-42".to_string(), "Fix the thing".to_string()),
            )]
            .into(),
            statuses: [
                ("st-1".to_string(), "In Progress".to_string()),
                ("st-2".to_string(), "In Review".to_string()),
            ]
            .into(),
            labels: [("lbl-1".to_string(), "bug".to_string())].into(),
        }
    }

    fn event(kind: &str, payload: serde_json::Value) -> EventRow {
        EventRow {
            id: format!("evt-{kind}"),
            issue_id: "issue-1".to_string(),
            team_id: Some("team-1".to_string()),
            created_at_ms: 0,
            kind: kind.to_string(),
            payload: Some(payload),
            board_id: Some("board-1".to_string()),
        }
    }

    fn armed(kind: TriggerKind) -> ArmedTrigger {
        ArmedTrigger {
            trigger_id: "trg-1".to_string(),
            agent_id: "agt-1".to_string(),
            team_id: "team-1".to_string(),
            trigger: ParsedTrigger { kind },
            fingerprint: "fp".to_string(),
        }
    }

    fn event_armed() -> ArmedTrigger {
        armed(TriggerKind::Event(EventSpec {
            event: EventKind::Created,
            board_ids: Vec::new(),
            label_ids: Vec::new(),
            priorities: Vec::new(),
            to_status_ids: Vec::new(),
        }))
    }

    /// One line per event kind — the grammar the agent reads.
    #[test]
    fn event_lines_render_per_kind() {
        let lookups = lookups();
        let line = |row: EventRow| event_line(&row, &lookups).expect("renders");
        assert_eq!(
            line(event(
                "status_changed",
                json!({
                    "from": "in_progress", "to": "in_review",
                    "fromStatusId": "st-1", "toStatusId": "st-2",
                    "fromName": "In Progress", "toName": "In Review"
                })
            )),
            "EXP-42 \"Fix the thing\" status In Progress → In Review"
        );
        // No name snapshot (an older row): the synced status rows fill in.
        assert_eq!(
            line(event(
                "status_changed",
                json!({"from": "in_progress", "to": "in_review",
                       "fromStatusId": "st-1", "toStatusId": "st-2"})
            )),
            "EXP-42 \"Fix the thing\" status In Progress → In Review"
        );
        // Neither: the anchor enum munges (the chain never fails).
        assert_eq!(
            line(event("status_changed", json!({"from": "in_progress", "to": "in_review"}))),
            "EXP-42 \"Fix the thing\" status in progress → in review"
        );
        assert_eq!(
            line(event("priority_changed", json!({"from": "low", "to": "urgent"}))),
            "EXP-42 \"Fix the thing\" priority low → urgent"
        );
        assert_eq!(
            line(event("created", json!({"priority": "urgent"}))),
            "EXP-42 \"Fix the thing\" created (urgent)"
        );
        // A cleared/absent priority reads `none`.
        assert_eq!(
            line(event("priority_changed", json!({"from": "low"}))),
            "EXP-42 \"Fix the thing\" priority low → none"
        );
        assert_eq!(
            line(event("label_added", json!({"labelId": "lbl-1"}))),
            "EXP-42 \"Fix the thing\" label bug added"
        );
        // An unsynced label degrades to its id rather than dropping the line.
        assert_eq!(
            line(event("label_added", json!({"labelId": "lbl-9"}))),
            "EXP-42 \"Fix the thing\" label lbl-9 added"
        );
        assert_eq!(
            line(event("assignee_changed", json!({}))),
            "EXP-42 \"Fix the thing\" assignee changed"
        );
        assert_eq!(
            line(event("pr_opened", json!({}))),
            "EXP-42 \"Fix the thing\" pull request opened"
        );
        assert_eq!(
            line(event("pr_merged", json!({}))),
            "EXP-42 \"Fix the thing\" pull request merged"
        );

        // An issue this device has not synced names nothing — skip the line.
        let mut orphan = event("created", json!({"priority": "urgent"}));
        orphan.issue_id = "issue-unknown".to_string();
        assert_eq!(event_line(&orphan, &lookups), None);

        // A multi-line title stays one list item.
        let mut messy = lookups.clone();
        messy.issues.insert(
            "issue-1".to_string(),
            ("EXP-42".to_string(), "Fix\nthe\0  thing".to_string()),
        );
        assert_eq!(
            event_line(&event("pr_merged", json!({})), &messy).unwrap(),
            "EXP-42 \"Fix the thing\" pull request merged"
        );
    }

    /// The firing messages are agent-facing copy — byte-lock them.
    #[test]
    fn firing_text_locks() {
        let schedule = armed(TriggerKind::Schedule(Schedule {
            interval: ScheduleInterval::Weekly,
            minute_of_day: 540,
            weekday: Some(1),
            day_of_month: None,
        }));
        assert_eq!(
            firing_text(&schedule, &Firing::Schedule { occurrence_ms: 0 }, &lookups()),
            "Trigger fired: your schedule (weekly on Monday at 09:00, device time)."
        );

        let firing = Firing::Event {
            matches: vec![
                event("created", json!({"priority": "urgent"})),
                event("label_added", json!({"labelId": "lbl-1"})),
            ],
        };
        assert_eq!(
            firing_text(&event_armed(), &firing, &lookups()),
            "Trigger fired: these issues changed:\n\
\n- EXP-42 \"Fix the thing\" created (urgent)\
\n- EXP-42 \"Fix the thing\" label bug added"
        );
    }

    /// Past the line cap, and for issues this device cannot name, the
    /// overflow folds into ONE "…and N more." tail.
    #[test]
    fn firing_text_caps_lines_and_counts_the_rest() {
        let mut matches: Vec<EventRow> = (0..FIRING_MAX_LINES + 3)
            .map(|_| event("pr_opened", json!({})))
            .collect();
        let mut orphan = event("pr_opened", json!({}));
        orphan.issue_id = "issue-unknown".to_string();
        matches.insert(0, orphan);
        let text = firing_text(&event_armed(), &Firing::Event { matches }, &lookups());
        assert_eq!(text.matches("pull request opened").count(), FIRING_MAX_LINES - 1);
        assert!(text.ends_with("\n- …and 5 more."), "{text}");

        // Nothing nameable at all still says it fired.
        let mut unknown = event("created", json!({}));
        unknown.issue_id = "issue-unknown".to_string();
        assert_eq!(
            firing_text(
                &event_armed(),
                &Firing::Event { matches: vec![unknown.clone(), unknown] },
                &lookups()
            ),
            "Trigger fired: 2 issue changes matched, on issues this device has not synced."
        );
    }

    /// Huge titles never push the message past the server's text cap.
    #[test]
    fn firing_text_stays_under_the_send_cap() {
        let mut lookups = lookups();
        lookups.issues.insert(
            "issue-1".to_string(),
            ("EXP-42".to_string(), "x".repeat(2_000)),
        );
        let matches: Vec<EventRow> = (0..20).map(|_| event("pr_merged", json!({}))).collect();
        let text = firing_text(&event_armed(), &Firing::Event { matches }, &lookups);
        assert!(text.encode_utf16().count() <= domain::contract::SESSIONS_SEND_TEXT_MAX);
        assert!(text.contains("more."), "the dropped lines are counted");
    }

    #[test]
    fn firing_issue_ids_are_distinct_and_capped() {
        let mut second = event("created", json!({}));
        second.issue_id = "issue-2".to_string();
        let firing = Firing::Event {
            matches: vec![event("created", json!({})), second, event("pr_merged", json!({}))],
        };
        assert_eq!(firing_issue_ids(&firing), vec!["issue-1", "issue-2"]);
        assert!(firing_issue_ids(&Firing::Schedule { occurrence_ms: 0 }).is_empty());
    }
}
