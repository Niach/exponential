package com.exponential.app.data

import com.exponential.app.data.db.BoardEntity
import com.exponential.app.data.db.IssueEntity
import com.exponential.app.data.db.IssueStatusEntity
import com.exponential.app.data.db.TeamEntity
import com.exponential.app.domain.IssueStatusCategory
import com.exponential.app.ui.chat.linkedIssueStatuses
import com.exponential.app.ui.markdown.chatTeamIssueRefTargets
import com.exponential.app.ui.components.teamBands
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

// EXP-1186: the cross-team lists band per team only with several teams, and a
// page opened from another team's row works in the ROW's team.
class CrossTeamTest {

    private fun team(id: String, name: String) = TeamEntity(
        id = id,
        name = name,
        slug = id,
        createdAt = "2026-10-01T00:00:00Z",
        updatedAt = "2026-10-01T00:00:00Z",
    )

    @Test
    fun `one team is one band with no team, several band in team order`() {
        val rows = listOf("b1" to "b", "a1" to "a", "b2" to "b", "x1" to "gone")
        val single = teamBands(rows, listOf(team("a", "Alpha"))) { it.second }
        assertEquals(1, single.size)
        assertNull(single.single().team)
        assertEquals(rows, single.single().items)

        val bands = teamBands(rows, listOf(team("a", "Alpha"), team("b", "Beta"))) { it.second }
        assertEquals(listOf("a", "b", null), bands.map { it.team?.id })
        assertEquals(listOf("b1", "b2"), bands[1].items.map { it.first })
        assertEquals(listOf("x1"), bands[2].items.map { it.first })
    }

    @Test
    fun `a saved prompt, then a pr, then the first issue names the route's subject`() {
        assertEquals(
            RouteSubject.Prompt("act-2"),
            routeSubject(mapOf("action" to "act-2", "issues" to "i-1")::get),
        )
        // A builtin has no team row: its pr pick decides.
        assertEquals(
            RouteSubject.Issue("i-pr"),
            routeSubject(mapOf("action" to "builtin:create-action", "pr" to "i-pr")::get),
        )
        assertEquals(RouteSubject.Issue("i-1"), routeSubject(mapOf("issues" to "i-1,i-2")::get))
        // The Chat FAB names nothing: the selected team stands.
        assertNull(routeSubject(emptyMap<String, String>()::get))
        assertNull(routeSubject(mapOf("action" to "builtin:create-action")::get))
        // EXP-1284: a start into a chat works in the CHAT's team; an
        // issue seed still wins.
        assertEquals(RouteSubject.Chat("t-1"), routeSubject(mapOf("chat" to "t-1")::get))
        assertEquals(RouteSubject.Issue("i-1"), routeSubject(mapOf("issues" to "i-1", "chat" to "t-1")::get))
    }

    private fun board(id: String, teamId: String) = BoardEntity(
        id = id,
        teamId = teamId,
        name = id,
        slug = id,
        prefix = "EXP",
        color = "#6366f1",
        sortOrder = 0.0,
        createdAt = "2026-01-01 00:00:00+00",
        updatedAt = "2026-01-01 00:00:00+00",
    )

    private fun issue(id: String, boardId: String, identifier: String, createdAt: String = "2026-11-01T00:00:00Z") = IssueEntity(
        id = id, boardId = boardId, number = 1, identifier = identifier, title = id,
        status = "backlog", priority = "none", creatorId = "u1", sortOrder = 1.0,
        createdAt = createdAt, updatedAt = createdAt,
    )

    private fun statusRow(id: String, teamId: String, name: String) = IssueStatusEntity(
        id = id,
        teamId = teamId,
        category = IssueStatusCategory.Backlog.wire,
        name = name,
        sortOrder = 0.0,
        createdAt = "2026-01-01 00:00:00+00",
        updatedAt = "2026-01-01 00:00:00+00",
    )

    // #EXP-2079 (agent-settings.json `assistant.chatTeams`).
    @Test
    fun `the cross-team order is by name case-insensitively, then id`() {
        val teams = listOf(team("t3", "beta"), team("t2", "Alpha"), team("t1", "alpha"), team("t4", "Beta"))
        assertEquals(listOf("t1", "t2", "t3", "t4"), crossTeamOrder(teams).map { it.id })
    }

    @Test
    fun `the chat teams' issues, an identifier found in several is the first team's`() {
        val boards = listOf(board("ba", "t-a"), board("bb", "t-b"), board("bc", "t-c"))
        val issues = listOf(
            issue("a1", "ba", "EXP-1"),
            issue("b1", "bb", "exp-1"),
            issue("b2", "bb", "EXP-2"),
            issue("c1", "bc", "EXP-3"),
        )
        // One team (every team chat): that team's issues, untouched.
        assertEquals(listOf("b1", "b2"), chatTeamIssues(issues, boards, listOf("t-b")).map { it.id })
        // Several: EXP-1 is the first team's in order, whichever that is.
        assertEquals(listOf("a1", "b2"), chatTeamIssues(issues, boards, listOf("t-a", "t-b")).map { it.id })
        assertEquals(listOf("b1", "b2"), chatTeamIssues(issues, boards, listOf("t-b", "t-a")).map { it.id })
        assertEquals(emptyList<String>(), chatTeamIssues(issues, boards, emptyList()).map { it.id })
    }

    @Test
    fun `a chat team issue's glyph reads its own team's statuses, newest first`() {
        val boards = listOf(board("ba", "t-a"), board("bb", "t-b"))
        val issues = listOf(
            issue("a1", "ba", "EXP-1", createdAt = "2026-11-01T00:00:00Z").copy(statusId = "sa"),
            issue("b2", "bb", "EXP-2", createdAt = "2026-11-02T00:00:00Z").copy(statusId = "sb"),
        )
        val rows = listOf(statusRow("sa", "t-a", "Inbox A"), statusRow("sb", "t-b", "Inbox B"))
        val targets = chatTeamIssueRefTargets(issues, boards, rows, listOf("t-a", "t-b"))
        assertEquals(listOf("b2", "a1"), targets.map { it.issueId })
        assertEquals(listOf("Inbox B", "Inbox A"), targets.map { it.resolvedStatus?.name })
        // A linked issue of another team than the chat's: its own team's too.
        assertEquals(
            mapOf("a1" to "Inbox A", "b2" to "Inbox B"),
            linkedIssueStatuses(issues, boards, rows).mapValues { it.value.name },
        )
    }
}
