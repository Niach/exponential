package com.exponential.app.ui.reviews

import com.exponential.app.data.api.OpenPull
import com.exponential.app.data.db.BoardEntity
import com.exponential.app.domain.PullRepo
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
            teams = listOf(team("team-1", "Team")),
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
            teams = listOf(team("team-1", "Team")),
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
            teams = listOf(team("team-1", "Team")),
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

    // EXP-1244: the unlinked pulls band per repository after the runs; a
    // pull an issue links never lists, and the empty state counts the bands.
    @Test
    fun repoBandsListOnlyUnlinkedPullsAndCountTowardsTheQueue() {
        val linked = "https://github.com/acme/app/pull/a"
        val state = buildReviewsState(
            issues = listOf(issue("a", prUrl = linked)),
            boards = listOf(board("board-1", 1.0)),
            runs = emptyList(),
            teams = listOf(team("team-1", "Team")),
            pulls = listOf(
                PullRepo(
                    teamId = "team-1",
                    repositoryId = "repo-1",
                    fullName = "acme/app",
                    pulls = listOf(
                        OpenPull(number = 3, url = "https://github.com/acme/app/pull/3", title = "Bump deps"),
                        OpenPull(number = 4, url = linked),
                    ),
                ),
            ),
        )
        val band = state.repoGroups.single()
        assertEquals(null, band.teamName)
        assertEquals(listOf(3), band.repo.pulls.map { it.number })
        assertEquals(2, state.count)

        val onlyPulls = buildReviewsState(
            issues = emptyList(),
            boards = emptyList(),
            runs = emptyList(),
            teams = listOf(team("team-1", "Team")),
            pulls = listOf(
                PullRepo("team-1", "repo-1", "acme/app", listOf(OpenPull(3, "https://github.com/acme/app/pull/3"))),
            ),
        )
        assertTrue(!onlyPulls.isEmpty)
    }

    // EXP-1248: the row label ×4 (web `reviewRowLabel`).
    @Test
    fun `names a single-issue PR by its issue`() {
        val state = buildReviewsState(
            issues = listOf(issue("a")),
            boards = listOf(board("board-1", 1.0)),
            runs = emptyList(),
            teams = listOf(team("team-1", "Team")),
        )
        assertEquals(ReviewRowLabel("A", "Issue a"), reviewRowLabel(state.groups.single().entries.single()))
    }

    @Test
    fun `names a batch PR by its first issue plus the rest`() {
        val shared = "https://github.com/acme/app/pull/7"
        val state = buildReviewsState(
            issues = listOf(
                issue("a", prUrl = shared, createdAt = "2026-09-10T10:00:00Z"),
                issue("b", prUrl = shared, createdAt = "2026-09-10T11:00:00Z"),
                issue("c", prUrl = shared, createdAt = "2026-09-10T09:00:00Z"),
            ),
            boards = listOf(board("board-1", 1.0)),
            runs = emptyList(),
            teams = listOf(team("team-1", "Team")),
        )
        assertEquals(ReviewRowLabel("B +2", "Issue b"), reviewRowLabel(state.groups.single().entries.single()))
    }

    // EXP-1248: a band's items as drawn ×4 (web `reviewBlocks`).
    @Test
    fun `keeps a tree in ONE list and gives each stack its own rail`() {
        fun stacked(id: String, base: String, createdAt: String) =
            issue(id, createdAt = createdAt).copy(prBaseBranch = base)
        val state = buildReviewsState(
            issues = listOf(
                // A tree: root r with two children.
                stacked("r", "master", "2026-09-10T12:00:00Z"),
                stacked("r1", "exp/R", "2026-09-10T11:00:00Z"),
                stacked("r2", "exp/R", "2026-09-10T10:00:00Z"),
                // A stack: s2 on s1.
                stacked("s2", "exp/S1", "2026-09-10T09:00:00Z"),
                stacked("s1", "main", "2026-09-10T08:00:00Z"),
            ),
            boards = listOf(board("board-1", 1.0)),
            runs = emptyList(),
            teams = listOf(team("team-1", "Team")),
        )
        val blocks = reviewBlocks(state.groups.single().items)
        assertEquals(2, blocks.size)
        val tree = blocks[0] as ReviewBlock.Rows
        assertEquals(listOf("r" to 0, "r1" to 1, "r2" to 1), tree.rows.map { it.entry.representative.id to it.depth })
        val stack = blocks[1] as ReviewBlock.Stack
        assertEquals(listOf("s2", "s1"), stack.entries.map { it.representative.id })
        assertEquals("main", stack.baseBranch)
    }
}
