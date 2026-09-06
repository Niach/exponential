package com.exponential.app.domain

import com.exponential.app.data.db.CodingSessionEntity
import com.exponential.app.data.db.IssueEntity

// EXP-746: the Devices screen's "Past" section — the caller's own FINISHED,
// person-started runs. Everything here is pure and mirrored ×4 (web
// `past-runs.ts`, iOS `PastRuns.swift`, desktop `devices_view.rs`); the row
// SELECTION lives in `AgentsViewModel.pastRunRows`, this file owns the copy.

/**
 * What a Past row is called: the issue's title, else — while that issue row
 * has not synced yet — "Issue syncing…", else the run's `action_name`
 * snapshot (which outlives the action, and is how a chat run reads "Chat",
 * EXP-615), else "Batch run".
 *
 * Byte-identical ×4 with web `pastRunTitle`, iOS `PastRuns.title` and desktop
 * `session_title`, so the same ended run is named the same on every client.
 * Locked by `a row titles itself from whatever it has`.
 */
fun pastRunTitle(session: CodingSessionEntity, issue: IssueEntity?): String = when {
    issue != null -> issue.title.trim().ifBlank { "Untitled issue" }
    session.issueId != null -> "Issue syncing…"
    else -> session.actionName?.trim()?.ifBlank { null } ?: "Batch run"
}

/**
 * How a run's `ended_by` reads in a byline. The wire vocabulary is the
 * contract's `agent | user | client | merge | system`; anything else (an older
 * or newer publisher) drops the clause rather than printing a raw token.
 *
 * Byte-identical ×4 — the whole byline is asserted by
 * `the past byline names device, agent and who ended it`.
 */
fun endedByPhrase(endedBy: String?): String? = when (endedBy) {
    "agent" -> "agent"
    "user" -> "you"
    "client" -> "the app"
    "merge" -> "a merge"
    "system" -> "the system"
    else -> null
}

/**
 * One Past row's byline: `<device> · <agent label> · ended by <who> · <time>`.
 *
 * Every part is optional — an old row may carry no agent, a swept row no
 * `ended_by` — and a missing one simply drops its segment, so the separators
 * never double up. Byte-identical ×4.
 */
fun pastRunByline(
    deviceLabel: String?,
    agentLabel: String?,
    endedBy: String?,
    timeLabel: String,
): String = listOfNotNull(
    deviceLabel?.takeIf { it.isNotBlank() },
    agentLabel?.takeIf { it.isNotBlank() },
    endedByPhrase(endedBy)?.let { "ended by $it" },
    timeLabel.takeIf { it.isNotBlank() },
).joinToString(" · ")
