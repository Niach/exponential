package com.exponential.app.domain

import com.exponential.app.data.db.CodingSessionEntity
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.mapNotNull
import kotlinx.coroutines.withTimeoutOrNull

// EXP-536: after ANY remote start the surface that sent it jumps into the
// live session screen instead of leaving a "watch it in Agents" chip behind.
// A start is only a COMMAND — the desktop inserts the coding_sessions row a
// moment later — so every caller waits for that row to sync in and matches it
// against what it just started. The matching rules are one shared object
// because all six Android start surfaces (Agents, Actions, issue list,
// issue detail, Reviews, Changes) must agree on them.

/** What a post-send session watch is looking for in the synced rows. */
sealed interface StartedRunKey {
    /** An action run — keyed by the display-name SNAPSHOT, never the action
     * id: the builtin "Create action" row carries `action_id` NULL. */
    data class Action(val actionName: String) : StartedRunKey

    /** A single-issue session — the desktop stamps `issue_id`. */
    data class Issue(val issueId: String) : StartedRunKey

    /** A batch run: `issue_id` NULL (it spans issues) and no action name.
     * Nothing narrower is available — a batch row is unmatchable by issue
     * server-side too — so the userId + startedAt cut carries the identity. */
    data object Batch : StartedRunKey

    companion object {
        /** The key for a Start-coding send: 1 id is a plain session, 2+ a batch. */
        fun forIssues(issueIds: List<String>): StartedRunKey? = when {
            issueIds.isEmpty() -> null
            issueIds.size >= 2 -> Batch
            else -> Issue(issueIds.first())
        }
    }
}

object StartedRunMatch {

    /** Clock-skew slack on the desktop-written `started_at` — a row stamped
     * slightly before the send must still count as ours. */
    const val SKEW_MS: Long = 120_000L

    /** How long a start may take to surface its row before the watch gives
     * up. Captions share it: one that died first would strand a late jump
     * with no explanation, one that outlived it would spin forever. */
    const val DEADLINE_MS: Long = 180_000L

    fun matches(
        session: CodingSessionEntity,
        key: StartedRunKey,
        userId: String,
        cutoffMs: Long,
    ): Boolean {
        if (session.userId != userId) return false
        // EXP-530: automation-started rows (started_reason non-null) are the
        // device's own doing — a user's pending start watch must never grab
        // one, even when the action name and timing line up.
        if (session.startedReason != null) return false
        if ((CodingSessionLiveness.parseEpochMs(session.startedAt) ?: 0L) < cutoffMs) return false
        return when (key) {
            is StartedRunKey.Action -> session.actionName == key.actionName
            is StartedRunKey.Issue -> session.issueId == key.issueId && session.actionName == null
            StartedRunKey.Batch -> session.issueId == null && session.actionName == null
        }
    }

    /**
     * Suspend until [sessions] carries the row for THIS start, or the
     * deadline passes (null). [sessions] is the caller's live coding_sessions
     * flow — already scoped to the account, so a switch mid-watch just stops
     * emitting.
     */
    suspend fun await(
        sessions: Flow<List<CodingSessionEntity>>,
        key: StartedRunKey,
        userId: String,
    ): String? {
        val cutoffMs = System.currentTimeMillis() - SKEW_MS
        return withTimeoutOrNull(DEADLINE_MS) {
            sessions.mapNotNull { rows ->
                rows.firstOrNull { matches(it, key, userId, cutoffMs) }
            }.first().id
        }
    }
}
