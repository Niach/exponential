package com.exponential.app.domain

import com.exponential.app.data.db.CodingSessionEntity
import com.exponential.app.data.db.IssueEntity
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

// EXP-897 part 4 — the badge's model, the same four tests web
// (`pr-graph.test.ts`), iOS (`PrGraphTests`) and the desktop (`pr_graph`) run.
class PrGraphTest {

    private fun issue(
        id: String,
        identifier: String = id.uppercase(),
        branch: String? = null,
        base: String? = null,
        prUrl: String? = null,
    ) = IssueEntity(
        id = id,
        boardId = "board-1",
        number = 1,
        identifier = identifier,
        title = "Issue $identifier",
        status = "backlog",
        priority = "none",
        sortOrder = 1.0,
        prUrl = prUrl,
        branch = branch,
        prBaseBranch = base,
        createdAt = "2026-09-10T10:00:00Z",
        updatedAt = "2026-09-10T10:00:00Z",
    )

    private fun session(id: String, parent: String? = null, issueId: String? = null) =
        CodingSessionEntity(
            id = id,
            issueId = issueId,
            teamId = "team-1",
            userId = "user-1",
            parentSessionId = parent,
            startedAt = "2026-09-10T10:00:00Z",
            createdAt = "2026-09-10T10:00:00Z",
            updatedAt = "2026-09-10T10:00:00Z",
        )

    private fun graph(
        issue: IssueEntity?,
        issues: List<IssueEntity>,
        session: CodingSessionEntity? = null,
        sessions: List<CodingSessionEntity> = emptyList(),
    ) = PrGraph.build(issue, session, issues, sessions, emptyList())

    @Test
    fun reportsAStackBadgeForAStackedPr() {
        val bottom = issue("a", branch = "exp/A")
        val top = issue("b", branch = "exp/B", base = "exp/A")
        val built = graph(top, listOf(bottom, top))

        assertEquals(PrGraph.BadgeKind.STACK, PrGraph.badgeKind(built))
        assertEquals(listOf("a", "b"), built.stack.map { it.entry.representative.id })
        assertEquals(listOf(0, 1), built.stack.map { it.depth })
        assertEquals(1, built.subject?.depth)
        assertNull(built.batch)
    }

    @Test
    fun reportsABatchBadgeForABatchPr() {
        val first = issue("a", prUrl = "https://github.com/o/r/pull/7", branch = "exp/batch-1")
        val second = issue("b", prUrl = "https://github.com/o/r/pull/7", branch = "exp/batch-1")
        val built = graph(first, listOf(first, second))

        assertEquals(PrGraph.BadgeKind.BATCH, PrGraph.badgeKind(built))
        assertEquals(listOf("a", "b"), built.batch?.issues?.map { it.id })
        assertEquals(1, built.stack.size)
    }

    @Test
    fun reportsBothForABatchInsideAStack() {
        val foundation = issue("f", branch = "exp/F")
        val first = issue("a", prUrl = "https://github.com/o/r/pull/7", branch = "exp/batch-1", base = "exp/F")
        val second = issue("b", prUrl = "https://github.com/o/r/pull/7", branch = "exp/batch-1", base = "exp/F")
        val built = graph(first, listOf(foundation, first, second))

        assertEquals(PrGraph.BadgeKind.STACK_AND_BATCH, PrGraph.badgeKind(built))
        // ONE node per pull request: the batch is a single stack member.
        assertEquals(2, built.stack.size)
        assertTrue(built.stack.last().entry.isBatch)
        assertEquals(listOf("a", "b"), built.batch?.issues?.map { it.id })
    }

    @Test
    fun reportsNothingForALonePr() {
        val lone = issue("a", branch = "exp/A", prUrl = "https://github.com/o/r/pull/3")
        val built = graph(lone, listOf(lone, issue("other", branch = "exp/O")))

        assertNull(PrGraph.badgeKind(built))
        assertEquals(1, built.stack.size)
        assertNull(built.batch)
    }

    @Test
    fun nestsTheRunFamilyOfTheShownSession() {
        val root = session("root")
        val child = session("child", parent = "root")
        val grand = session("grand", parent = "child")
        val stranger = session("stranger")
        val built = graph(
            null,
            emptyList(),
            session = child,
            sessions = listOf(stranger, grand, child, root),
        )
        assertEquals(listOf("root", "child", "grand"), built.tree.map { it.session.id })
        assertEquals(listOf(0, 1, 2), built.tree.map { it.depth })
    }
}
