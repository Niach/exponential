//! Human-readable trigger sentences (EXP-530), shared by the desktop GUI
//! and the CLI so both surfaces speak identical copy. [`schedule_phrase`]
//! is lowercase (it embeds mid-sentence in the trigger prompt section);
//! [`trigger_summary`] is the standalone list-row sentence.

use super::trigger::{ParsedTrigger, Schedule, ScheduleInterval, TriggerKind};

const WEEKDAY_NAMES: [&str; 7] = [
    "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday",
];

/// `daily at 07:00` / `weekly on Monday at 09:00` / `monthly on day 5 at
/// 09:00` — lowercase, prompt-embeddable.
pub fn schedule_phrase(schedule: &Schedule) -> String {
    let time = format!(
        "{:02}:{:02}",
        schedule.minute_of_day / 60,
        schedule.minute_of_day % 60
    );
    match schedule.interval {
        ScheduleInterval::Daily => format!("daily at {time}"),
        ScheduleInterval::Weekly => {
            // The parser guarantees 1..=7 for weekly; a hand-built value
            // degrades to Monday rather than panicking a render pass.
            let name = WEEKDAY_NAMES
                [schedule.weekday.unwrap_or(1).clamp(1, 7) as usize - 1];
            format!("weekly on {name} at {time}")
        }
        ScheduleInterval::Monthly => {
            format!("monthly on day {} at {time}", schedule.day_of_month.unwrap_or(1))
        }
    }
}

/// The one-line trigger sentence a list row shows.
pub fn trigger_summary(trigger: &ParsedTrigger) -> String {
    match &trigger.kind {
        TriggerKind::Schedule(schedule) => {
            let phrase = schedule_phrase(schedule);
            // Capitalize the leading interval word (ASCII by construction).
            let mut chars = phrase.chars();
            match chars.next() {
                Some(first) => format!("{}{}", first.to_ascii_uppercase(), chars.as_str()),
                None => phrase,
            }
        }
        TriggerKind::Event(spec) => {
            use super::trigger::EventKind;
            let sentence = match spec.event {
                EventKind::Created => "When an issue is created",
                EventKind::StatusChanged => "When status changes",
                EventKind::AssigneeChanged => "When the assignee changes",
                EventKind::LabelAdded => "When a label is added",
                EventKind::PriorityChanged => "When priority changes",
                EventKind::PrOpened => "When a pull request is opened",
                EventKind::PrMerged => "When a pull request is merged",
            };
            let filter_count = spec.board_ids.len()
                + spec.label_ids.len()
                + spec.priorities.len()
                + spec.to_status_ids.len();
            if filter_count > 0 {
                format!("{sentence} · {filter_count} filters")
            } else {
                sentence.to_string()
            }
        }
        TriggerKind::Unsupported => "Unsupported trigger — update the app".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::automations::trigger::{EventKind, EventSpec};

    fn trigger(kind: TriggerKind) -> ParsedTrigger {
        ParsedTrigger {
            device_id: "dev-1".to_string(),
            enabled: true,
            kind,
        }
    }

    /// The sentences are cross-surface copy — byte-lock them.
    #[test]
    fn schedule_phrases_lock() {
        assert_eq!(
            schedule_phrase(&Schedule {
                interval: ScheduleInterval::Daily,
                minute_of_day: 420,
                weekday: None,
                day_of_month: None,
            }),
            "daily at 07:00"
        );
        assert_eq!(
            schedule_phrase(&Schedule {
                interval: ScheduleInterval::Weekly,
                minute_of_day: 540,
                weekday: Some(1),
                day_of_month: None,
            }),
            "weekly on Monday at 09:00"
        );
        assert_eq!(
            schedule_phrase(&Schedule {
                interval: ScheduleInterval::Weekly,
                minute_of_day: 0,
                weekday: Some(7),
                day_of_month: None,
            }),
            "weekly on Sunday at 00:00"
        );
        assert_eq!(
            schedule_phrase(&Schedule {
                interval: ScheduleInterval::Monthly,
                minute_of_day: 540,
                weekday: None,
                day_of_month: Some(5),
            }),
            "monthly on day 5 at 09:00"
        );
    }

    #[test]
    fn trigger_summaries_lock() {
        assert_eq!(
            trigger_summary(&trigger(TriggerKind::Schedule(Schedule {
                interval: ScheduleInterval::Daily,
                minute_of_day: 420,
                weekday: None,
                day_of_month: None,
            }))),
            "Daily at 07:00"
        );
        assert_eq!(
            trigger_summary(&trigger(TriggerKind::Schedule(Schedule {
                interval: ScheduleInterval::Monthly,
                minute_of_day: 540,
                weekday: None,
                day_of_month: Some(5),
            }))),
            "Monthly on day 5 at 09:00"
        );

        let mut spec = EventSpec {
            event: EventKind::Created,
            board_ids: Vec::new(),
            label_ids: Vec::new(),
            priorities: Vec::new(),
            to_status_ids: Vec::new(),
        };
        assert_eq!(
            trigger_summary(&trigger(TriggerKind::Event(spec.clone()))),
            "When an issue is created"
        );
        spec.board_ids = vec!["b-1".to_string(), "b-2".to_string()];
        spec.priorities = vec!["urgent".to_string()];
        assert_eq!(
            trigger_summary(&trigger(TriggerKind::Event(spec.clone()))),
            "When an issue is created · 3 filters"
        );
        for (event, sentence) in [
            (EventKind::StatusChanged, "When status changes"),
            (EventKind::AssigneeChanged, "When the assignee changes"),
            (EventKind::LabelAdded, "When a label is added"),
            (EventKind::PriorityChanged, "When priority changes"),
            (EventKind::PrOpened, "When a pull request is opened"),
            (EventKind::PrMerged, "When a pull request is merged"),
        ] {
            spec.event = event;
            spec.board_ids = Vec::new();
            spec.priorities = Vec::new();
            assert_eq!(trigger_summary(&trigger(TriggerKind::Event(spec.clone()))), sentence);
        }

        assert_eq!(
            trigger_summary(&trigger(TriggerKind::Unsupported)),
            "Unsupported trigger — update the app"
        );
    }
}
