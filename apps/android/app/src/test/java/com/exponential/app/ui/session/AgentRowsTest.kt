package com.exponential.app.ui.session

import com.exponential.app.data.db.CodingSessionEntity
import com.exponential.app.data.db.IssueEntity
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

// EXP-312 follow-up: the Agents list is OWNER-ONLY. A teammate's live session
// cannot be viewed or steered, so listing it only read as "computer not
// online" — it stays synced (issue-detail badges, Reviews) but never rows here.
// It is also SELECTED-TEAM-only, matching web's use-agents-data: an own run in
// another team belongs under that team, not under whichever one is open.
class AgentRowsTest {

    // 2026-07-17T12:00:00Z
    private val nowMs = 1_784_289_600_000L

    private fun session(
        id: String,
        userId: String,
        issueId: String? = "issue-1",
        teamId: String = "team-1",
        status: String = "running",
        updatedAt: String = "2026-07-17T11:30:00Z",
    ) = CodingSessionEntity(
        id = id,
        issueId = issueId,
        teamId = teamId,
        userId = userId,
        status = status,
        startedAt = "2026-07-17T09:00:00Z",
        createdAt = "2026-07-17T09:00:00Z",
        updatedAt = updatedAt,
    )

    private fun issue(id: String) = IssueEntity(
        id = id,
        boardId = "board-1",
        number = 1,
        identifier = "EXP-1",
        title = "An issue",
        status = "in_progress",
        priority = "none",
        sortOrder = 1.0,
        createdAt = "2026-07-17T09:00:00Z",
        updatedAt = "2026-07-17T09:00:00Z",
    )

    @Test
    fun `lists only the signed-in user's own sessions`() {
        val rows = agentRows(
            sessions = listOf(
                session("mine", userId = "me"),
                session("theirs", userId = "teammate"),
            ),
            issues = listOf(issue("issue-1")),
            currentUserId = "me",
            teamId = "team-1",
            nowMs = nowMs,
        )
        assertEquals(listOf("mine"), rows.map { it.session.id })
    }

    @Test
    fun `empty when every live session belongs to someone else`() {
        val rows = agentRows(
            sessions = listOf(
                session("theirs", userId = "teammate"),
                session("also-theirs", userId = "other"),
            ),
            issues = listOf(issue("issue-1")),
            currentUserId = "me",
            teamId = "team-1",
            nowMs = nowMs,
        )
        assertEquals(emptyList<AgentRow>(), rows)
    }

    @Test
    fun `signed out lists nothing`() {
        val rows = agentRows(
            sessions = listOf(session("mine", userId = "me")),
            issues = listOf(issue("issue-1")),
            currentUserId = null,
            teamId = "team-1",
            nowMs = nowMs,
        )
        assertEquals(emptyList<AgentRow>(), rows)
    }

    @Test
    fun `own session in another team is not listed`() {
        val rows = agentRows(
            sessions = listOf(
                session("here", userId = "me"),
                session("elsewhere", userId = "me", teamId = "team-2"),
            ),
            issues = listOf(issue("issue-1")),
            currentUserId = "me",
            teamId = "team-1",
            nowMs = nowMs,
        )
        assertEquals(listOf("here"), rows.map { it.session.id })
    }

    @Test
    fun `no selected team lists nothing`() {
        val rows = agentRows(
            sessions = listOf(session("mine", userId = "me")),
            issues = listOf(issue("issue-1")),
            currentUserId = "me",
            teamId = null,
            nowMs = nowMs,
        )
        assertEquals(emptyList<AgentRow>(), rows)
    }

    @Test
    fun `own stale session still drops out`() {
        // The EXP-153 staleness cut applies on top of the ownership filter.
        val rows = agentRows(
            sessions = listOf(session("mine", userId = "me", updatedAt = "2026-07-17T09:00:00Z")),
            issues = listOf(issue("issue-1")),
            currentUserId = "me",
            teamId = "team-1",
            nowMs = nowMs,
        )
        assertEquals(emptyList<AgentRow>(), rows)
    }

    @Test
    fun `own batch session lists without an issue link`() {
        // Batch rows carry no issue but DO carry an explicit team_id, so the
        // team scoping keeps them.
        val rows = agentRows(
            sessions = listOf(session("batch", userId = "me", issueId = null)),
            issues = listOf(issue("issue-1")),
            currentUserId = "me",
            teamId = "team-1",
            nowMs = nowMs,
        )
        assertEquals(1, rows.size)
        assertNull(rows.single().issue)
    }

    @Test
    fun `own session joins its issue`() {
        val rows = agentRows(
            sessions = listOf(session("mine", userId = "me")),
            issues = listOf(issue("issue-1")),
            currentUserId = "me",
            teamId = "team-1",
            nowMs = nowMs,
        )
        assertEquals("EXP-1", rows.single().issue?.identifier)
    }
}
