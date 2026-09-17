package com.exponential.app.domain

import com.exponential.app.data.db.CodingSessionEntity
import com.exponential.app.data.db.IssueEntity

// EXP-746: the Agent page's "Recent" section — the caller's own FINISHED,
// person-started runs. Everything here is pure and mirrored ×4 (web
// `past-runs.ts`, iOS `PastRuns.swift`, desktop `devices_view.rs`); the row
// SELECTION lives in `AgentsViewModel.pastRunRows`, this file owns the copy.
// EXP-886: the session screen's run switcher (`issueRunRows`) reuses the
// byline with `issueRunWhen` in its time slot.

/**
 * EXP-888: the staleness sweep's end — `ended_by = "stale"`. The server flips
 * a silent row to `ended` only when its host device advertises the
 * `stale-end` cap; that device IGNORES the flip and its next heartbeat (up to
 * 30 min later) revives the row to `running`. So a sweep end says "silent for
 * the staleness window", NEVER "this run is over". ×4 (web `runIsStaleEnd`,
 * desktop `run_rows::run_is_stale_end`, iOS `PastRuns.isStaleEnd`).
 */
fun runIsStaleEnd(session: CodingSessionEntity): Boolean =
    session.status == DomainContract.codingSessionStatusEnded &&
        session.endedBy == DomainContract.codingSessionEndedByStale

/**
 * THE predicate: whether the run is OVER. [runIsLive] is its negation. Every
 * Running-vs-Past, Stop-vs-Resume and composer-enabled decision goes through
 * this and never through `status == "ended"` alone — or a swept-but-alive run
 * lists as past, greys its composer out and offers a Resume that would put a
 * SECOND agent on the same worktree. Byte-identical ×4 (web `runHasEnded`,
 * desktop `run_rows::run_has_ended`, iOS `PastRuns.hasEnded`).
 */
fun runHasEnded(session: CodingSessionEntity): Boolean =
    session.status == DomainContract.codingSessionStatusEnded && !runIsStaleEnd(session)

/** The negation of [runHasEnded] — the run may still be alive. ×4. */
fun runIsLive(session: CodingSessionEntity): Boolean = !runHasEnded(session)

/** A run that is alive by STATUS — running or in review. Staleness is not
 *  consulted: a run whose machine went quiet is still one of the issue's
 *  runs, it only sorts by its last heartbeat. ×4. */
fun isLiveRunStatus(status: String): Boolean =
    status == DomainContract.codingSessionStatusRunning ||
        status == DomainContract.codingSessionStatusInReview

/** The row-level twin of [isLiveRunStatus]: live by status, plus EXP-888's
 *  sweep end, which is a live run whose heartbeat has not landed yet. Every
 *  list that paints a live badge or sorts live runs first reads THIS. ×4 (web
 *  `isLiveRun`, desktop `queries::is_live_run_status`, iOS `PastRuns.isLiveRun`). */
fun isLiveRun(session: CodingSessionEntity): Boolean =
    runIsStaleEnd(session) || isLiveRunStatus(session.status)

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
    if (isLiveRun(session)) LIVE_RUN_LABEL else endedRelative

/**
 * What a Past row is called: the issue's title, else — while that issue row
 * has not synced yet — "Issue syncing…", else the run's `action_name`
 * snapshot (which outlives the action, and is how a chat run reads "Chat",
 * EXP-615), else the batch's own name (EXP-876: its first covered issue's
 * title, else "Batch run").
 *
 * Byte-identical ×4 with web `pastRunTitle`, iOS `PastRuns.title` and desktop
 * `session_title`, so the same ended run is named the same on every client.
 * Locked by `a row titles itself from whatever it has`.
 */
fun pastRunTitle(
    session: CodingSessionEntity,
    issue: IssueEntity?,
    batchIssues: List<IssueEntity> = emptyList(),
): String = when {
    issue != null -> issue.title.trim().ifBlank { "Untitled issue" }
    session.issueId != null -> "Issue syncing…"
    else ->
        chatRunSubject(session)
            ?: session.actionName?.trim()?.ifBlank { null }
            ?: batchRunName(session, batchIssues).subject
}

/** The hidden Chat builtin's name — the `action_name` snapshot every chat run
 *  carries (EXP-615; byte-identical with `builtinChatAction`). */
const val CHAT_RUN_NAME = "Chat"

/**
 * EXP-905: a CHAT run's subject — the agent CLI's own auto-title
 * (`agent_title`, trimmed) when non-empty, else "Chat". Null for every other
 * run (issue/action/batch keep their names). A chat run = no issue, no action
 * row, `action_name` == "Chat". Mirrored ×4.
 */
fun chatRunSubject(session: CodingSessionEntity): String? {
    if (session.issueId != null || session.actionId != null || session.actionName != CHAT_RUN_NAME) return null
    return session.agentTitle?.trim()?.ifBlank { null } ?: CHAT_RUN_NAME
}

/**
 * EXP-876: the row's mono lead-in — the issue's identifier, a batch's
 * `EXP-874 +2`, else none. The twin of [pastRunTitle], ×4 lockstep (web
 * `pastRunIdentifier`).
 */
fun pastRunIdentifier(
    session: CodingSessionEntity,
    issue: IssueEntity?,
    batchIssues: List<IssueEntity> = emptyList(),
): String? = when {
    issue != null -> issue.identifier
    session.issueId != null || session.actionName != null -> null
    else -> batchRunName(session, batchIssues).identifier
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
