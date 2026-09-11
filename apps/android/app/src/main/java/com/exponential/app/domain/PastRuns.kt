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
 * One Past row's byline: `<device> · <time>`.
 *
 * Both parts are optional — an old row may name no device, a swept row has no
 * honest time — and a missing one simply drops its segment, so the separator
 * never dangles. EXP-833 dropped the agent label and the "ended by" clause:
 * the right side had grown wider than the titles, and the agent already
 * shows as the row's lead glyph. Byte-identical ×4, asserted by
 * `the past byline names the device and when it ended`.
 */
fun pastRunByline(
    deviceLabel: String?,
    timeLabel: String,
): String = listOfNotNull(
    deviceLabel?.takeIf { it.isNotBlank() },
    timeLabel.takeIf { it.isNotBlank() },
).joinToString(" · ")
