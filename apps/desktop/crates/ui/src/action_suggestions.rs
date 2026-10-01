//! Curated action-suggestion seeds (EXP-530) — the desktop mirror of the web
//! `lib/action-suggestions.ts`. The Suggestions tab renders them as cards and
//! "Use" opens the create-action dialog with the description
//! prefilled, so each description is written as INSTRUCTIONS for what the
//! authored action's prompt should do: it becomes the builtin "Create action"
//! run's description input and the creator agent acts on it verbatim.
//!
//! EXP-583: a seed may also carry a `trigger` — the suggested when-part.
//! Such seeds append the machine-readable trigger block to the creator run's
//! request (SLOP-2: `crate::trigger_editor::format_trigger_block`) and the
//! creator run sets up both; their row wears that trigger's glyph. Seeds
//! without one suggest a plain action.
//!
//! The strings are CROSS-CLIENT copy — keep them byte-identical to the web
//! seeds (`ACTION_SUGGESTIONS` there); [`suggestions_mirror_the_web_seeds`]
//! locks the shape, the ids and the curated-icon rule.

use coding::automations::{EventKind, ScheduleInterval};
use serde_json::{json, Value};

/// A seed's suggested trigger, in the const-friendly shape a static table can
/// hold. [`SuggestedTrigger::to_trigger`] renders the wire JSON.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SuggestedTrigger {
    Schedule {
        interval: ScheduleInterval,
        minute_of_day: u32,
        /// 1=Monday..7=Sunday — required for (and only read by) weekly.
        weekday: Option<u32>,
    },
    Event {
        event: EventKind,
    },
}

impl SuggestedTrigger {
    /// The wire trigger, key-ordered like the web seeds so the appended
    /// creator-run block matches byte for byte.
    pub(crate) fn to_trigger(self) -> Value {
        match self {
            Self::Schedule {
                interval,
                minute_of_day,
                weekday,
            } => {
                let mut trigger = json!({
                    "kind": "schedule",
                    "interval": match interval {
                        ScheduleInterval::Daily => "daily",
                        ScheduleInterval::Weekly => "weekly",
                        ScheduleInterval::Monthly => "monthly",
                    },
                    "minuteOfDay": minute_of_day,
                });
                if let Some(weekday) = weekday {
                    trigger["weekday"] = json!(weekday);
                }
                trigger
            }
            Self::Event { event } => json!({"kind": "event", "event": event.wire()}),
        }
    }
}

/// One suggestion card. `icon` is a name from the curated `boardIcon`
/// registry set — never a raw glyph (the swatch grid and the natives only
/// know that set).
#[derive(Clone, Copy, Debug)]
pub(crate) struct Suggestion {
    pub id: &'static str,
    pub title: &'static str,
    pub description: &'static str,
    pub icon: &'static str,
    /// `Some` = the seed suggests this trigger with its action: the row
    /// wears its glyph and "Use" appends the trigger block.
    pub trigger: Option<SuggestedTrigger>,
}

pub(crate) const ACTION_SUGGESTIONS: &[Suggestion] = &[
    Suggestion {
        id: "daily-standup-digest",
        title: "Daily standup digest",
        description: "Summarize what changed across the team's boards in the last 24 hours: \
                      issues created, completed, and moved, plus open pull requests. Post the \
                      digest as a comment on a dedicated standup issue, grouped by board with \
                      issue identifiers linked.",
        icon: "calendar",
        trigger: Some(SuggestedTrigger::Schedule {
            interval: ScheduleInterval::Daily,
            minute_of_day: 540,
            weekday: None,
        }),
    },
    Suggestion {
        id: "label-new-issues",
        title: "Label new issues",
        description: "When an issue is created, read its title and description and apply the \
                      best-fitting existing labels. Never create new labels, never change other \
                      fields, and leave no comment unless no label fits at all.",
        icon: "target",
        trigger: Some(SuggestedTrigger::Event {
            event: EventKind::Created,
        }),
    },
    Suggestion {
        id: "backlog-grooming-sweep",
        title: "Backlog grooming sweep",
        description: "Review backlog issues and flag ones that are missing a description, a \
                      priority, or labels. Leave a short comment on each flagged issue listing \
                      what is missing, and skip issues already flagged in a previous run.",
        icon: "boxes",
        trigger: None,
    },
    Suggestion {
        id: "stale-issue-nudge",
        title: "Stale-issue nudge",
        description: "Find in-progress issues with no updates for 14 days or more. Comment on \
                      each one asking the assignee for a status update, mentioning them by \
                      email, and include how long the issue has been quiet.",
        icon: "clock",
        trigger: Some(SuggestedTrigger::Schedule {
            interval: ScheduleInterval::Weekly,
            minute_of_day: 540,
            weekday: Some(1),
        }),
    },
    Suggestion {
        id: "release-notes-drafter",
        title: "Release notes drafter",
        description: "Collect issues completed since the last run and draft user-facing release \
                      notes from their titles and descriptions. Group entries into Features, \
                      Fixes, and Improvements, and file the draft as a new issue for review.",
        icon: "file-text",
        trigger: None,
    },
    Suggestion {
        id: "pr-review-summarizer",
        title: "PR review summarizer",
        description: "For each open pull request linked to a team issue, read the changed files \
                      and post a concise review summary as a comment on the linked issue: what \
                      the change does, notable risks, and suggested test focus areas.",
        icon: "git-branch",
        trigger: None,
    },
    Suggestion {
        id: "widget-bug-triage",
        title: "Bug triage on new widget feedback",
        description: "Triage newly created widget-reported issues: rewrite vague titles to be \
                      specific, set a priority based on severity, add reproduction steps when \
                      they can be inferred from the report, and label likely duplicates.",
        icon: "bug",
        trigger: Some(SuggestedTrigger::Event {
            event: EventKind::Created,
        }),
    },
    Suggestion {
        id: "weekly-metrics-comment",
        title: "Weekly metrics comment",
        description: "Compute weekly team metrics: issues created versus completed, average time \
                      to done, and open pull request count. Post the numbers with a \
                      week-over-week comparison as a comment on a dedicated metrics issue.",
        icon: "chart-line",
        trigger: Some(SuggestedTrigger::Schedule {
            interval: ScheduleInterval::Weekly,
            minute_of_day: 480,
            weekday: Some(5),
        }),
    },
    Suggestion {
        id: "label-janitor",
        title: "Label janitor",
        description: "Keep labels tidy: find unlabeled issues and apply the best-fitting existing \
                      labels based on title and description. Never create new labels, and list \
                      every change made in a summary comment on a dedicated janitor issue.",
        icon: "flag",
        trigger: None,
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    /// The web seeds are the source of truth — this locks the 9 ids in
    /// order, the curated-icon rule and the "instructions, not marketing"
    /// shape of every description.
    #[test]
    fn suggestions_mirror_the_web_seeds() {
        let ids: Vec<&str> = ACTION_SUGGESTIONS.iter().map(|entry| entry.id).collect();
        assert_eq!(
            ids,
            vec![
                "daily-standup-digest",
                "label-new-issues",
                "backlog-grooming-sweep",
                "stale-issue-nudge",
                "release-notes-drafter",
                "pr-review-summarizer",
                "widget-bug-triage",
                "weekly-metrics-comment",
                "label-janitor",
            ]
        );
        for entry in ACTION_SUGGESTIONS {
            assert!(!entry.title.is_empty(), "{} needs a title", entry.id);
            // Icons come from the curated set the swatch grid renders — a raw
            // lucide name here would draw a fallback glyph everywhere.
            assert!(
                domain::contract::BOARD_ICON_VALUES.contains(&entry.icon),
                "{} icon {} is not a curated boardIcon",
                entry.id,
                entry.icon
            );
            // Prefilled verbatim into the creator run's description input.
            assert!(
                entry.description.len() > 80 && entry.description.ends_with('.'),
                "{} needs an instruction-shaped description",
                entry.id
            );
            assert!(
                !entry.description.contains("  "),
                "{} has a double space (line-continuation slip)",
                entry.id
            );
            // Every suggested trigger must survive the shared parser — its
            // glyph and the appended block both read it.
            if let Some(suggested) = entry.trigger {
                let trigger = suggested.to_trigger();
                let parsed = coding::automations::parse_trigger(&trigger)
                    .unwrap_or_else(|| panic!("{} trigger must parse", entry.id));
                assert_ne!(
                    parsed.kind,
                    coding::automations::TriggerKind::Unsupported,
                    "{} trigger must be representable",
                    entry.id
                );
            }
        }
    }

    /// EXP-583: which seeds carry a trigger, and the exact when-part each
    /// one suggests — byte-locked against the web seeds.
    #[test]
    fn trigger_seeds_match_the_web_triggers() {
        let by_id = |id: &str| {
            ACTION_SUGGESTIONS
                .iter()
                .find(|entry| entry.id == id)
                .unwrap_or_else(|| panic!("{id} exists"))
        };
        let trigger = |id: &str| by_id(id).trigger.map(SuggestedTrigger::to_trigger);

        assert_eq!(
            trigger("daily-standup-digest"),
            Some(json!({"kind": "schedule", "interval": "daily", "minuteOfDay": 540}))
        );
        assert_eq!(
            trigger("label-new-issues"),
            Some(json!({"kind": "event", "event": "created"}))
        );
        assert_eq!(
            trigger("stale-issue-nudge"),
            Some(json!({
                "kind": "schedule", "interval": "weekly", "minuteOfDay": 540, "weekday": 1
            }))
        );
        assert_eq!(
            trigger("widget-bug-triage"),
            Some(json!({"kind": "event", "event": "created"}))
        );
        assert_eq!(
            trigger("weekly-metrics-comment"),
            Some(json!({
                "kind": "schedule", "interval": "weekly", "minuteOfDay": 480, "weekday": 5
            }))
        );
        // The four plain seeds suggest no trigger.
        for id in [
            "backlog-grooming-sweep",
            "release-notes-drafter",
            "pr-review-summarizer",
            "label-janitor",
        ] {
            assert_eq!(trigger(id), None, "{id} is a plain action seed");
        }
    }
}
