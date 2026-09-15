package com.exponential.app.domain

import com.exponential.app.data.db.CodingSessionEntity
import com.exponential.app.data.db.IssueEntity

// EXP-746: the Agent page's "Recent" section — the caller's own FINISHED,
// person-started runs. Everything here is pure and mirrored ×4 (web
// `past-runs.ts`, iOS `PastRuns.swift`, desktop `devices_view.rs`); the row
// SELECTION lives in `AgentsViewModel.pastRunRows`, this file owns the copy.
// EXP-886: the session screen's run switcher (`issueRunRows`) reuses the
// byline with `issueRunWhen` in its time slot.

/** A run that is alive by STATUS — running or in review. Staleness is not
 *  consulted: a run whose machine went quiet is still one of the issue's
 *  runs, it only sorts by its last heartbeat. ×4. */
fun isLiveRunStatus(status: String): Boolean =
    status == DomainContract.codingSessionStatusRunning ||
        status == DomainContract.codingSessionStatusInReview

/** The word the run switcher shows in a live run's time slot, in place of the
 *  ended relative time. Byte-identical ×4 (web `LIVE_RUN_LABEL`). */
const val LIVE_RUN_LABEL = "Live"

/**
 * EXP-886: the switcher's `<when>` for a run — [LIVE_RUN_LABEL] for a
 * live-status run, else the caller's already-formatted ended time. The entry
 * is [pastRunByline] over it; the trigger names the run on show by it. ×4
 * (web `issueRunWhen`).
 */
fun issueRunWhen(session: CodingSessionEntity, endedRelative: String): String =
    if (isLiveRunStatus(session.status)) LIVE_RUN_LABEL else endedRelative

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
