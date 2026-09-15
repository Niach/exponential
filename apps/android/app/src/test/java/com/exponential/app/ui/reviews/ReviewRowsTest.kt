package com.exponential.app.ui.reviews

import com.exponential.app.data.db.IssueEntity
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

// EXP-897: Reviews nests a stacked pull request under the one it is based on
// (`pr_base_branch` → the lower entry's `branch`), the whole stack groups
// under its ROOT's board, and only the bottom row offers "Merge stack".
class ReviewRowsTest {

    private fun issue(
        id: String,
        identifier: String = id.uppercase(),
        boardId: String = "board-1",
        branch: String? = null,
        base: String? = null,
    ) = IssueEntity(
        id = id,
        boardId = boardId,
        number = 1,
        identifier = identifier,
        title = "Issue $identifier",
        status = "in_review",
        priority = "none",
        sortOrder = 1.0,
        prState = "open",
        branch = branch,
        prBaseBranch = base,
        createdAt = "2026-09-10T10:00:00Z",
        updatedAt = "2026-09-10T10:00:00Z",
    )

    private fun entry(vararg issues: IssueEntity) = ReviewEntry(
        groupKey = issues.first().prUrl ?: "issue:${issues.first().id}",
        prUrl = issues.first().prUrl,
        prNumber = issues.first().prNumber,
        branch = issues.first().branch,
        boardId = issues.first().boardId,
        issues = issues.toList(),
    )

    @Test
    fun nestsAStackedPullRequestUnderItsFoundation() {
        val rows = buildReviewRows(
            listOf(
                entry(issue("top", branch = "exp/TOP", base = "exp/BOTTOM")),
                entry(issue("lone", branch = "exp/LONE")),
                entry(issue("bottom", branch = "exp/BOTTOM", base = "master")),
            ),
        )
        assertEquals(listOf("lone", "bottom", "top"), rows.map { it.entry.representative.id })
        assertEquals(listOf(0, 0, 1), rows.map { it.depth })
        assertEquals(listOf(false, true, false), rows.map { it.hasChildren })
        assertEquals(listOf(null, null, "BOTTOM"), rows.map { it.stackedOn })
    }

    @Test
    fun offersMergeStackOnTheBottomRowOnly() {
        val rows = buildReviewRows(
            listOf(
                entry(issue("bottom", branch = "exp/BOTTOM")),
                entry(issue("middle", branch = "exp/MIDDLE", base = "exp/BOTTOM")),
                entry(issue("top", branch = "exp/TOP", base = "exp/MIDDLE")),
            ),
        )
        assertEquals(listOf("bottom", 3), listOf(rows[0].entry.representative.id, rows[0].stackSize))
        assertEquals("bottom", rows[0].mergeStackIssueId)
        assertNull(rows[1].mergeStackIssueId)
        assertNull(rows[2].mergeStackIssueId)
        // A lone pull request is not a stack, however deep the list is.
        val lone = buildReviewRows(listOf(entry(issue("a", branch = "exp/A"))))
        assertNull(lone.single().mergeStackIssueId)
        assertEquals(1, lone.single().stackSize)
    }

    @Test
    fun keepsAWholeStackUnderTheRootsBoard() {
        val rows = buildReviewRows(
            listOf(
                entry(issue("bottom", boardId = "board-1", branch = "exp/BOTTOM")),
                entry(issue("top", boardId = "board-2", branch = "exp/TOP", base = "exp/BOTTOM")),
            ),
        )
        assertEquals(listOf("board-1", "board-1"), rows.map { it.rootBoardId })
    }
}
