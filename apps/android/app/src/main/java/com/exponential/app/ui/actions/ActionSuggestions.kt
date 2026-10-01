package com.exponential.app.ui.actions

import com.exponential.app.domain.AutomationTrigger

// EXP-530 Suggestions tab: 9 seed action ideas, mirrored per client from
// apps/web/src/lib/action-suggestions.ts — id/title/description/icon must
// stay in lockstep with the web list (icons come from the shared pickable
// registry). A tap opens the composer prefilled with the
// description + icon; the creator agent authors the real action from there.
//
// EXP-583 / SLOP-2: a seed may carry a [trigger] — the suggested when-part.
// Such a seed wears that trigger's glyph beside its title and appends the
// machine-readable trigger block to the creator run's request (bound to the
// caller's default trigger-capable machine), so the creator run sets up both
// the action and its trigger. Seeds without one are plain suggestions.

data class ActionSuggestion(
    val id: String,
    val title: String,
    val description: String,
    /** Curated registry icon name (the `boardIconValues` set). */
    val icon: String,
    /** The suggested when-part trigger; null = a plain action suggestion. */
    val trigger: AutomationTrigger? = null,
)

val ACTION_SUGGESTIONS: List<ActionSuggestion> = listOf(
    ActionSuggestion(
        id = "daily-standup-digest",
        title = "Daily standup digest",
        description = "Summarize what changed across the team's boards in the last 24 hours: " +
            "issues created, completed, and moved, plus open pull requests. Post the digest " +
            "as a comment on a dedicated standup issue, grouped by board with issue " +
            "identifiers linked.",
        icon = "calendar",
        trigger = AutomationTrigger.Schedule(interval = "daily", minuteOfDay = 540),
    ),
    ActionSuggestion(
        id = "label-new-issues",
        title = "Label new issues",
        description = "When an issue is created, read its title and description and apply the " +
            "best-fitting existing labels. Never create new labels, never change other " +
            "fields, and leave no comment unless no label fits at all.",
        icon = "target",
        trigger = AutomationTrigger.Event(event = "created"),
    ),
    ActionSuggestion(
        id = "backlog-grooming-sweep",
        title = "Backlog grooming sweep",
        description = "Review backlog issues and flag ones that are missing a description, a " +
            "priority, or labels. Leave a short comment on each flagged issue listing what " +
            "is missing, and skip issues already flagged in a previous run.",
        icon = "boxes",
    ),
    ActionSuggestion(
        id = "stale-issue-nudge",
        title = "Stale-issue nudge",
        description = "Find in-progress issues with no updates for 14 days or more. Comment on " +
            "each one asking the assignee for a status update, mentioning them by email, and " +
            "include how long the issue has been quiet.",
        icon = "clock",
        trigger = AutomationTrigger.Schedule(
            interval = "weekly",
            minuteOfDay = 540,
            weekday = 1,
        ),
    ),
    ActionSuggestion(
        id = "release-notes-drafter",
        title = "Release notes drafter",
        description = "Collect issues completed since the last run and draft user-facing " +
            "release notes from their titles and descriptions. Group entries into Features, " +
            "Fixes, and Improvements, and file the draft as a new issue for review.",
        icon = "file-text",
    ),
    ActionSuggestion(
        id = "pr-review-summarizer",
        title = "PR review summarizer",
        description = "For each open pull request linked to a team issue, read the changed " +
            "files and post a concise review summary as a comment on the linked issue: what " +
            "the change does, notable risks, and suggested test focus areas.",
        icon = "git-branch",
    ),
    ActionSuggestion(
        id = "widget-bug-triage",
        title = "Bug triage on new widget feedback",
        description = "Triage newly created widget-reported issues: rewrite vague titles to be " +
            "specific, set a priority based on severity, add reproduction steps when they " +
            "can be inferred from the report, and label likely duplicates.",
        icon = "bug",
        trigger = AutomationTrigger.Event(event = "created"),
    ),
    ActionSuggestion(
        id = "weekly-metrics-comment",
        title = "Weekly metrics comment",
        description = "Compute weekly team metrics: issues created versus completed, average " +
            "time to done, and open pull request count. Post the numbers with a " +
            "week-over-week comparison as a comment on a dedicated metrics issue.",
        icon = "chart-line",
        trigger = AutomationTrigger.Schedule(
            interval = "weekly",
            minuteOfDay = 480,
            weekday = 5,
        ),
    ),
    ActionSuggestion(
        id = "label-janitor",
        title = "Label janitor",
        description = "Keep labels tidy: find unlabeled issues and apply the best-fitting " +
            "existing labels based on title and description. Never create new labels, and " +
            "list every change made in a summary comment on a dedicated janitor issue.",
        icon = "flag",
    ),
)
