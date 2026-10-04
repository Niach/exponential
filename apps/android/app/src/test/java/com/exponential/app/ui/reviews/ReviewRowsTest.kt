package com.exponential.app.ui.reviews

import com.exponential.app.data.db.BoardEntity
import com.exponential.app.data.db.CodingSessionEntity
import com.exponential.app.data.db.IssueEntity
import com.exponential.app.data.db.TeamEntity
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

// SLOP-3: Reviews is a FLAT list of open pull requests per board, deduped by
// `pr_url` (a batch PR linking several issues is ONE entry); the "Agent runs"
// group holds only the run PRs no issue carries.
class ReviewRowsTest {

    private fun issue(
        id: String,
        boardId: String = "board-1",
        prUrl: String? = "https://github.com/acme/app/pull/$id",
        createdAt: String = "2026-09-10T10:00:00Z",
    ) = IssueEntity(
        id = id,
        boardId = boardId,
        number = 1,
        identifier = id.uppercase(),
        title = "Issue $id",
        status = "in_review",
        priority = "none",
        sortOrder = 1.0,
        prUrl = prUrl,
        prState = "open",
        branch = "exp/${id.uppercase()}",
        createdAt = createdAt,
        updatedAt = createdAt,
    )

    private fun board(id: String, sortOrder: Double, teamId: String = "team-1") = BoardEntity(
        id = id,
        teamId = teamId,
        name = "Board $id",
        slug = id,
        prefix = "EXP",
        color = "#888888",
        sortOrder = sortOrder,
        createdAt = "2026-09-10T10:00:00Z",
        updatedAt = "2026-09-10T10:00:00Z",
    )

    private fun run(
        id: String,
        prUrl: String,
        startedAt: String = "2026-09-10T10:00:00Z",
        teamId: String = "team-1",
    ) =
        CodingSessionEntity(
            id = id,
            issueId = null,
            teamId = teamId,
            userId = "me",
            status = "in_review",
            actionName = "Deploy",
            prUrl = prUrl,
            prNumber = 9,
            prState = "open",
            startedAt = startedAt,
            createdAt = startedAt,
            updatedAt = startedAt,
        )

    @Test
    fun collapsesABatchPullRequestIntoOneEntry() {
        val shared = "https://github.com/acme/app/pull/7"
        val state = buildReviewsState(
            issues = listOf(
                issue("a", prUrl = shared, createdAt = "2026-09-10T10:00:00Z"),
                issue("b", prUrl = shared, createdAt = "2026-09-10T11:00:00Z"),
                issue("c"),
            ),
            boards = listOf(board("board-1", 1.0)),
            runs = emptyList(),
        )
        val entries = state.groups.single().entries
        assertEquals(2, entries.size)
        val batch = entries.single { it.prUrl == shared }
        assertTrue(batch.isBatch)
        // The newest linked issue represents the batch.
        assertEquals("b", batch.representative.id)
        assertEquals(listOf("B", "A"), batch.identifiers)
    }

    @Test
    fun listsEntriesFlatNewestFirstUnderTheirBoardInBoardOrder() {
        val state = buildReviewsState(
            issues = listOf(
                issue("old", boardId = "board-2", createdAt = "2026-09-10T09:00:00Z"),
                issue("new", boardId = "board-2", createdAt = "2026-09-10T12:00:00Z"),
                issue("other", boardId = "board-1"),
                issue("gone", boardId = "board-unsynced"),
            ),
            boards = listOf(board("board-2", 2.0), board("board-1", 1.0)),
            runs = emptyList(),
        )
        assertEquals(listOf("board-1", "board-2"), state.groups.map { it.board.id })
        assertEquals(listOf("new", "old"), state.groups[1].entries.map { it.representative.id })
    }

    @Test
    fun agentRunsHoldOnlyThePullRequestsNoIssueCarries() {
        val batchPr = "https://github.com/acme/app/pull/7"
        val chorePr = "https://github.com/acme/app/pull/8"
        val state = buildReviewsState(
            issues = listOf(issue("a", prUrl = batchPr), issue("b", prUrl = batchPr)),
            boards = listOf(board("board-1", 1.0)),
            runs = listOf(
                // A batch run stamps the combined PR on its own row too.
                run("batch-run", batchPr),
                run("chore-run", chorePr),
                // A resume continues the same PR: one entry, the newest row.
                run("chore-resume", chorePr, startedAt = "2026-09-10T12:00:00Z"),
            ),
        )
        assertEquals(listOf("chore-resume"), state.runs.map { it.session.id })
        assertEquals(1, state.groups.single().entries.size)
    }

    private fun team(id: String, name: String) = TeamEntity(
        id = id,
        name = name,
        slug = id,
        createdAt = "2026-09-10T10:00:00Z",
        updatedAt = "2026-09-10T10:00:00Z",
    )

    // EXP-1186: cross-team — boards order by team (name order) then board
    // order, each band names its team, and run PRs band once per team.
    @Test
    fun crossTeamBandsBoardsByTeamAndNamesTheTeamOnlyWithSeveral() {
        val teams = listOf(team("team-a", "Alpha"), team("team-b", "Beta"))
        val state = buildReviewsState(
            issues = listOf(
                issue("b1", boardId = "beta-board"),
                issue("a2", boardId = "alpha-2"),
                issue("a1", boardId = "alpha-1"),
            ),
            boards = listOf(
                board("beta-board", 0.5, teamId = "team-b"),
                board("alpha-2", 2.0, teamId = "team-a"),
                board("alpha-1", 1.0, teamId = "team-a"),
            ),
            runs = listOf(
                run("beta-run", "https://github.com/acme/app/pull/20", teamId = "team-b"),
                run("alpha-run", "https://github.com/acme/app/pull/21", teamId = "team-a"),
            ),
            teams = teams,
        )
        assertEquals(listOf("alpha-1", "alpha-2", "beta-board"), state.groups.map { it.board.id })
        assertEquals(listOf("Alpha", "Alpha", "Beta"), state.groups.map { it.teamName })
        assertEquals(listOf("team-a", "team-b"), state.runGroups.map { it.team?.id })

        // One team = today's list: no team names, one run band.
        val single = buildReviewsState(
            issues = listOf(issue("a1", boardId = "alpha-1")),
            boards = listOf(board("alpha-1", 1.0, teamId = "team-a")),
            runs = listOf(run("alpha-run", "https://github.com/acme/app/pull/21", teamId = "team-a")),
            teams = teams.take(1),
        )
        assertEquals(listOf<String?>(null), single.groups.map { it.teamName })
        assertEquals(listOf<String?>(null), single.runGroups.map { it.team?.id })
    }
}
