package com.exponential.app.domain

import com.exponential.app.data.db.BoardEntity
import com.exponential.app.data.db.IssueDraftEntity
import com.exponential.app.data.db.IssueStatusEntity
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * EXP-878: the "Drafts" section joins the static per-user issue_drafts shape
 * to the synced boards. A draft whose board does not resolve is hidden, a null
 * `status_id` renders the team's Backlog, and the list is newest-edited first.
 */
class DraftRowsTest {

    private fun board(id: String, teamId: String = "team-1", name: String = "Core") = BoardEntity(
        id = id,
        teamId = teamId,
        name = name,
        slug = id,
        prefix = "EXP",
        color = "#6366f1",
        sortOrder = 0.0,
        createdAt = "2026-01-01 00:00:00+00",
        updatedAt = "2026-01-01 00:00:00+00",
    )

    private fun draft(
        id: String,
        boardId: String = "b1",
        statusId: String? = null,
        updatedAt: String = "2026-09-14 10:00:00+00",
    ) = IssueDraftEntity(
        id = id,
        userId = "user-1",
        teamId = "team-1",
        boardId = boardId,
        title = "draft $id",
        statusId = statusId,
        createdAt = "2026-09-14 09:00:00+00",
        updatedAt = updatedAt,
    )

    private fun statusRow(id: String, teamId: String, name: String) = IssueStatusEntity(
        id = id,
        teamId = teamId,
        category = IssueStatusCategory.Started.wire,
        name = name,
        sortOrder = 0.0,
        createdAt = "2026-01-01 00:00:00+00",
        updatedAt = "2026-01-01 00:00:00+00",
    )

    @Test
    fun `a draft whose board is missing is dropped`() {
        val rows = resolveDraftRows(
            drafts = listOf(draft("d1", boardId = "b1"), draft("gone", boardId = "missing")),
            boards = listOf(board("b1")),
            statusesByTeam = emptyMap(),
        )
        assertEquals(listOf("d1"), rows.map { it.draft.id })
        assertEquals("Core", rows.single().board.name)
    }

    @Test
    fun `a null status_id renders the team Backlog builtin`() {
        val rows = resolveDraftRows(
            drafts = listOf(draft("d1")),
            boards = listOf(board("b1")),
            statusesByTeam = emptyMap(),
        )
        assertEquals(IssueStatus.Backlog, rows.single().status.builtinKey)
        assertEquals(IssueStatusCategory.Backlog, rows.single().status.category)
    }

    @Test
    fun `a status_id resolves against ITS OWN team's rows`() {
        val rows = resolveDraftRows(
            drafts = listOf(draft("d1", boardId = "b1", statusId = "s-team-1")),
            boards = listOf(board("b1", teamId = "team-1")),
            statusesByTeam = mapOf(
                "team-1" to IssueStatusResolver.teamStatuses(
                    listOf(statusRow("s-team-1", "team-1", "Reviewing")),
                ),
                "team-2" to IssueStatusResolver.teamStatuses(
                    listOf(statusRow("s-team-2", "team-2", "Elsewhere")),
                ),
            ),
        )
        assertEquals("Reviewing", rows.single().status.name)
    }

    @Test
    fun `rows come back newest-edited first`() {
        val rows = resolveDraftRows(
            drafts = listOf(
                draft("older", updatedAt = "2026-09-13 10:00:00+00"),
                draft("newest", updatedAt = "2026-09-14 18:00:00+00"),
                draft("middle", updatedAt = "2026-09-14 08:00:00+00"),
            ),
            boards = listOf(board("b1")),
            statusesByTeam = emptyMap(),
        )
        assertEquals(listOf("newest", "middle", "older"), rows.map { it.draft.id })
    }

    @Test
    fun `no drafts is no rows`() {
        assertTrue(
            resolveDraftRows(emptyList(), listOf(board("b1")), emptyMap()).isEmpty(),
        )
    }
}
