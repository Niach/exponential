package com.exponential.app.domain

import com.exponential.app.data.db.CodingSessionEntity
import com.exponential.app.data.db.IssueEntity
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

// EXP-1165: a run's Merge target. Web `session-merge-target.test.ts`.
class MergeTargetTest {

    private val url = "https://github.com/o/r/pull/7"

    private fun issue(
        id: String,
        prUrl: String? = null,
        prState: String? = null,
    ) = IssueEntity(
        id = id,
        boardId = "board-1",
        number = 1,
        identifier = id.uppercase(),
        title = "Issue $id",
        status = "in_review",
        priority = "none",
        sortOrder = 1.0,
        prUrl = prUrl,
        prState = prState,
        createdAt = "2026-09-10T10:00:00Z",
        updatedAt = "2026-09-10T10:00:00Z",
    )

    private fun session(
        id: String,
        issueId: String? = null,
        batchIssueIds: String? = null,
        actionName: String? = null,
        prUrl: String? = url,
        prState: String? = "open",
    ) = CodingSessionEntity(
        id = id,
        issueId = issueId,
        teamId = "team-1",
        userId = "user-1",
        batchIssueIds = batchIssueIds,
        actionName = actionName,
        prUrl = prUrl,
        prNumber = 7,
        prState = prState,
        startedAt = "2026-09-10T10:00:00Z",
        createdAt = "2026-09-10T10:00:00Z",
        updatedAt = "2026-09-10T10:00:00Z",
    )

    @Test
    fun batchWithCarrierMergesThroughTheIssue() {
        val run = session("batch", batchIssueIds = """["a","b"]""")
        val covered = listOf(issue("a"), issue("b", prUrl = url, prState = "open"))
        assertEquals(MergeTarget.Issue("b"), resolveMergeTarget(run, null, covered))
        assertEquals("b", batchMergeCarrier(run, covered)?.id)
    }

    @Test
    fun batchWithoutCarrierMergesTheSession() {
        val run = session("batch", batchIssueIds = """["a","b"]""")
        val covered = listOf(issue("a"), issue("b", prUrl = "https://github.com/o/r/pull/8", prState = "open"))
        assertEquals(MergeTarget.Session("batch"), resolveMergeTarget(run, null, covered))
        assertEquals(MergeTarget.Session("batch"), resolveMergeTarget(run, null))
    }

    @Test
    fun batchWithMergedCarrierMergesTheSession() {
        val run = session("batch", batchIssueIds = """["a"]""")
        val covered = listOf(issue("a", prUrl = url, prState = "merged"))
        assertEquals(MergeTarget.Session("batch"), resolveMergeTarget(run, null, covered))
        assertNull(batchMergeCarrier(run, covered))
    }

    @Test
    fun issueRunIsUnchanged() {
        val run = session("run", issueId = "a", prUrl = null, prState = null)
        val own = issue("a", prUrl = url, prState = "open")
        val other = issue("b", prUrl = url, prState = "open")
        assertEquals(MergeTarget.Issue("a"), resolveMergeTarget(run, own, listOf(other)))
        assertNull(resolveMergeTarget(run, issue("a", prUrl = url, prState = "merged"), listOf(other)))
        assertNull(batchMergeCarrier(run, listOf(other)))
    }

    @Test
    fun runWithoutAPrHasNoTarget() {
        val run = session("batch", batchIssueIds = """["a"]""", prUrl = null, prState = null)
        assertNull(resolveMergeTarget(run, null, listOf(issue("a", prUrl = url, prState = "open"))))
    }
}
