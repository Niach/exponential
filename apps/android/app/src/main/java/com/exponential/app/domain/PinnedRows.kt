package com.exponential.app.domain

import com.exponential.app.data.db.ActionEntity
import com.exponential.app.data.db.CodingSessionEntity
import com.exponential.app.data.db.IssueEntity
import com.exponential.app.data.db.PinEntity

/**
 * One row of the "Pinned" section (EXP-778) — a synced pin joined to its
 * locally resolved target. The pins shape is static per user and NOT
 * team/trash scoped, so the join is where a pin on an unsynced target (a
 * trashed board's issue, another team's action, a session that has not
 * landed yet) drops out: a pin without a target is hidden, never a
 * placeholder row.
 */
sealed interface PinnedRow {
    val pin: PinEntity

    data class Issue(override val pin: PinEntity, val issue: IssueEntity) : PinnedRow

    data class Session(
        override val pin: PinEntity,
        val session: CodingSessionEntity,
        /** The linked issue when the run is issue-scoped and synced. */
        val issue: IssueEntity?,
    ) : PinnedRow

    data class Action(override val pin: PinEntity, val action: ActionEntity) : PinnedRow
}

/**
 * Resolve [pins] (already the active team's, in `sort_order` order) against
 * the synced targets. Rows keep the pins' order; a pin whose `kind` does not
 * match the id it carries, or whose target is absent, is skipped.
 */
fun resolvePinnedRows(
    pins: List<PinEntity>,
    issues: List<IssueEntity>,
    sessions: List<CodingSessionEntity>,
    actions: List<ActionEntity>,
): List<PinnedRow> {
    if (pins.isEmpty()) return emptyList()
    val issuesById = issues.associateBy { it.id }
    val sessionsById = sessions.associateBy { it.id }
    val actionsById = actions.associateBy { it.id }
    return pins.mapNotNull { pin ->
        when (pin.kind) {
            DomainContract.pinKindIssue ->
                pin.issueId?.let(issuesById::get)?.let { PinnedRow.Issue(pin, it) }
            DomainContract.pinKindSession ->
                pin.sessionId?.let(sessionsById::get)?.let { session ->
                    PinnedRow.Session(pin, session, session.issueId?.let(issuesById::get))
                }
            DomainContract.pinKindAction ->
                pin.actionId?.let(actionsById::get)?.let { PinnedRow.Action(pin, it) }
            else -> null
        }
    }
}
