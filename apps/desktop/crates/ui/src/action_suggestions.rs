//! Curated action-suggestion seeds (EXP-530) — the desktop mirror of the web
//! `lib/action-suggestions.ts`. The Suggestions tab renders them as cards and
//! "Use suggestion" opens the create-action dialog with the description
//! prefilled, so each description is written as INSTRUCTIONS for what the
//! authored action's prompt should do: it becomes the builtin "Create action"
//! run's description input and the creator agent acts on it verbatim.
//!
//! The strings are CROSS-CLIENT copy — keep them byte-identical to the web
//! seeds (`ACTION_SUGGESTIONS` there); [`suggestions_mirror_the_web_seeds`]
//! locks the shape, the ids and the curated-icon rule.

/// One suggestion card. `icon` is a name from the curated `boardIcon`
/// registry set — never a raw glyph (the swatch grid and the natives only
/// know that set).
#[derive(Clone, Copy, Debug)]
pub(crate) struct Suggestion {
    pub id: &'static str,
    pub title: &'static str,
    pub description: &'static str,
    pub icon: &'static str,
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
    },
    Suggestion {
        id: "backlog-grooming-sweep",
        title: "Backlog grooming sweep",
        description: "Review backlog issues and flag ones that are missing a description, a \
                      priority, or labels. Leave a short comment on each flagged issue listing \
                      what is missing, and skip issues already flagged in a previous run.",
        icon: "boxes",
    },
    Suggestion {
        id: "stale-issue-nudge",
        title: "Stale-issue nudge",
        description: "Find in-progress issues with no updates for 14 days or more. Comment on \
                      each one asking the assignee for a status update, mentioning them by \
                      email, and include how long the issue has been quiet.",
        icon: "clock",
    },
    Suggestion {
        id: "release-notes-drafter",
        title: "Release notes drafter",
        description: "Collect issues completed since the last run and draft user-facing release \
                      notes from their titles and descriptions. Group entries into Features, \
                      Fixes, and Improvements, and file the draft as a new issue for review.",
        icon: "file-text",
    },
    Suggestion {
        id: "pr-review-summarizer",
        title: "PR review summarizer",
        description: "For each open pull request linked to a team issue, read the changed files \
                      and post a concise review summary as a comment on the linked issue: what \
                      the change does, notable risks, and suggested test focus areas.",
        icon: "git-branch",
    },
    Suggestion {
        id: "widget-bug-triage",
        title: "Bug triage on new widget feedback",
        description: "Triage newly created widget-reported issues: rewrite vague titles to be \
                      specific, set a priority based on severity, add reproduction steps when \
                      they can be inferred from the report, and label likely duplicates.",
        icon: "bug",
    },
    Suggestion {
        id: "weekly-metrics-comment",
        title: "Weekly metrics comment",
        description: "Compute weekly team metrics: issues created versus completed, average time \
                      to done, and open pull request count. Post the numbers with a \
                      week-over-week comparison as a comment on a dedicated metrics issue.",
        icon: "chart-line",
    },
    Suggestion {
        id: "label-janitor",
        title: "Label janitor",
        description: "Keep labels tidy: find unlabeled issues and apply the best-fitting existing \
                      labels based on title and description. Never create new labels, and list \
                      every change made in a summary comment on a dedicated janitor issue.",
        icon: "flag",
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    /// The web seeds are the source of truth — this locks the 8 ids in
    /// order, the curated-icon rule and the "instructions, not marketing"
    /// shape of every description.
    #[test]
    fn suggestions_mirror_the_web_seeds() {
        let ids: Vec<&str> = ACTION_SUGGESTIONS.iter().map(|entry| entry.id).collect();
        assert_eq!(
            ids,
            vec![
                "daily-standup-digest",
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
        }
    }
}
