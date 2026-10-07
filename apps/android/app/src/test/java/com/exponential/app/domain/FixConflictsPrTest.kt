package com.exponential.app.domain

import com.exponential.app.data.db.IssueEntity
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/**
 * EXP-1233: the Fix merge conflicts builtin's own look — the branch line the
 * card's PR row shows and the pull request a `pr` pick resolves to (×4 with
 * web `branchLine` / `resolveFixConflictsPr`).
 */
class FixConflictsPrTest {

    private fun issue(
        id: String,
        identifier: String,
        prUrl: String? = "https://github.com/acme/app/pull/2117",
        prState: String? = "open",
        prNumber: Int? = 2117,
        branch: String? = "exp/$identifier",
        prBaseBranch: String? = "master",
    ) = IssueEntity(
        id = id,
        boardId = "board-1",
        number = 1,
        identifier = identifier,
        title = "Issue $identifier",
        status = "in_review",
        priority = "none",
        sortOrder = 1.0,
        prUrl = prUrl,
        prNumber = prNumber,
        prState = prState,
        branch = branch,
        prBaseBranch = prBaseBranch,
        createdAt = "2026-10-07T10:00:00Z",
        updatedAt = "2026-10-07T10:00:00Z",
    )

    @Test
    fun `branch line names the base when known`() {
        assertEquals("exp/APP-14 → master", branchLine("exp/APP-14", "master"))
        assertEquals("exp/APP-14", branchLine("exp/APP-14", null))
        assertEquals("exp/APP-14", branchLine("exp/APP-14", ""))
        assertEquals("", branchLine(null, "master"))
        assertEquals("", branchLine("", null))
    }

    @Test
    fun `nothing picked or not synced resolves to null`() {
        val rows = listOf(issue("i-1", "APP-14"))
        assertNull(resolveFixConflictsPr(null, rows))
        assertNull(resolveFixConflictsPr("", rows))
        assertNull(resolveFixConflictsPr("missing", rows))
    }

    @Test
    fun `a single PR carries its number, branches and issue`() {
        val rep = issue("i-1", "APP-14")
        val pr = resolveFixConflictsPr("i-1", listOf(rep, issue("i-9", "APP-9", prUrl = "https://x/pull/1")))!!
        assertEquals("i-1", pr.issueId)
        assertEquals(2117, pr.prNumber)
        assertEquals("exp/APP-14", pr.branch)
        assertEquals("master", pr.baseBranch)
        assertEquals(listOf("APP-14"), pr.issues.map { it.identifier })
    }

    @Test
    fun `a batch PR lists every open issue sharing its url, by identifier`() {
        val rows = listOf(
            issue("i-3", "APP-30"),
            issue("i-1", "APP-14"),
            issue("i-2", "APP-21"),
            // Same url but no longer open: not part of the pick.
            issue("i-4", "APP-40", prState = "merged"),
            // Another pull request entirely.
            issue("i-5", "APP-50", prUrl = "https://github.com/acme/app/pull/9"),
        )
        val pr = resolveFixConflictsPr("i-2", rows)!!
        assertEquals("i-2", pr.issueId)
        assertEquals(listOf("APP-14", "APP-21", "APP-30"), pr.issues.map { it.identifier })
    }

    @Test
    fun `no url keeps the representative alone`() {
        val rep = issue("i-1", "APP-14", prUrl = null, prBaseBranch = null)
        val pr = resolveFixConflictsPr("i-1", listOf(rep, issue("i-2", "APP-15", prUrl = null)))!!
        assertEquals(listOf("APP-14"), pr.issues.map { it.identifier })
        assertNull(pr.baseBranch)
    }
}
