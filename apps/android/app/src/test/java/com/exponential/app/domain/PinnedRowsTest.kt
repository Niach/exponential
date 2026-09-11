package com.exponential.app.domain

import com.exponential.app.data.db.ActionEntity
import com.exponential.app.data.db.CodingSessionEntity
import com.exponential.app.data.db.IssueEntity
import com.exponential.app.data.db.PinEntity
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * EXP-778: the "Pinned" section joins the static per-user pins shape to the
 * team-scoped targets. A pin without a resolvable target is hidden, the
 * pins' own order is kept, and a session pin carries its linked issue.
 */
class PinnedRowsTest {

    private fun issue(id: String) = IssueEntity(
        id = id,
        boardId = "b1",
        number = 1,
        identifier = "EXP-1",
        title = "t",
        status = "backlog",
        priority = "none",
        sortOrder = 0.0,
        createdAt = "2026-01-01 00:00:00+00",
        updatedAt = "2026-01-01 00:00:00+00",
    )

    private fun session(id: String, issueId: String?) = CodingSessionEntity(
        id = id,
        issueId = issueId,
        teamId = "team-1",
        userId = "user-1",
        startedAt = "2026-07-17T09:00:00Z",
        createdAt = "2026-07-17T09:00:00Z",
        updatedAt = "2026-07-17T09:00:00Z",
    )

    private fun action(id: String) = ActionEntity(
        id = id,
        teamId = "team-1",
        name = "Release",
        sortOrder = 0.0,
        createdAt = "2026-07-17T09:00:00Z",
        updatedAt = "2026-07-17T09:00:00Z",
    )

    private fun pin(
        id: String,
        kind: String,
        issueId: String? = null,
        sessionId: String? = null,
        actionId: String? = null,
        sortOrder: Double,
    ) = PinEntity(
        id = id,
        userId = "user-1",
        teamId = "team-1",
        kind = kind,
        issueId = issueId,
        sessionId = sessionId,
        actionId = actionId,
        sortOrder = sortOrder,
    )

    @Test
    fun `rows keep the pins order and resolve every kind`() {
        val rows = resolvePinnedRows(
            pins = listOf(
                pin("p1", DomainContract.pinKindAction, actionId = "a1", sortOrder = 1.0),
                pin("p2", DomainContract.pinKindIssue, issueId = "i1", sortOrder = 2.0),
                pin("p3", DomainContract.pinKindSession, sessionId = "s1", sortOrder = 3.0),
            ),
            issues = listOf(issue("i1")),
            sessions = listOf(session("s1", issueId = "i1")),
            actions = listOf(action("a1")),
        )
        assertEquals(listOf("p1", "p2", "p3"), rows.map { it.pin.id })
        assertTrue(rows[0] is PinnedRow.Action)
        assertTrue(rows[1] is PinnedRow.Issue)
        val sessionRow = rows[2] as PinnedRow.Session
        assertEquals("s1", sessionRow.session.id)
        assertEquals("i1", sessionRow.issue?.id)
    }

    @Test
    fun `an unresolvable target hides the pin`() {
        val rows = resolvePinnedRows(
            pins = listOf(
                pin("gone-issue", DomainContract.pinKindIssue, issueId = "missing", sortOrder = 1.0),
                pin("gone-session", DomainContract.pinKindSession, sessionId = "missing", sortOrder = 2.0),
                pin("gone-action", DomainContract.pinKindAction, actionId = "missing", sortOrder = 3.0),
                pin("kept", DomainContract.pinKindIssue, issueId = "i1", sortOrder = 4.0),
            ),
            issues = listOf(issue("i1")),
            sessions = emptyList(),
            actions = emptyList(),
        )
        assertEquals(listOf("kept"), rows.map { it.pin.id })
    }

    @Test
    fun `a kind that does not match its id or is unknown is skipped`() {
        val rows = resolvePinnedRows(
            pins = listOf(
                // kind says issue but only a session id is set.
                pin("mismatch", DomainContract.pinKindIssue, sessionId = "s1", sortOrder = 1.0),
                pin("unknown", "bookmark", issueId = "i1", sortOrder = 2.0),
            ),
            issues = listOf(issue("i1")),
            sessions = listOf(session("s1", issueId = null)),
            actions = emptyList(),
        )
        assertTrue(rows.isEmpty())
    }

    @Test
    fun `a batch session pin carries no issue`() {
        val rows = resolvePinnedRows(
            pins = listOf(pin("p", DomainContract.pinKindSession, sessionId = "s1", sortOrder = 1.0)),
            issues = emptyList(),
            sessions = listOf(session("s1", issueId = null)),
            actions = emptyList(),
        )
        assertEquals(null, (rows.single() as PinnedRow.Session).issue)
    }
}
