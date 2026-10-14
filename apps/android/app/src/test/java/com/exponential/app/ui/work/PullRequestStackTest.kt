package com.exponential.app.ui.work

import com.exponential.app.data.db.IssueEntity
import com.exponential.app.data.db.PullRequestEntity
import com.exponential.app.data.db.PullRequestIssueEntity
import com.exponential.app.domain.DomainContract
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class PullRequestStackTest {
    private fun issue(id: String, identifier: String, title: String) = IssueEntity(
        id = id, boardId = "b1", number = 1, identifier = identifier, title = title,
        status = "backlog", priority = "none", creatorId = "u1", sortOrder = 1.0,
        createdAt = "", updatedAt = "",
    )

    private fun pr(id: String, number: Int, base: String? = null, state: String = DomainContract.prStateOpen) =
        PullRequestEntity(id = id, number = number, head = "exp/$id", base = "main", basePullRequestId = base, state = state)

    @Test
    fun `a lone open PR has no stack`() {
        assertNull(PullRequestStack.stackConfirm(pr("a", 1), listOf(pr("a", 1))))
    }

    @Test
    fun `merge stack from the middle lands the whole chain through its top`() {
        val all = listOf(pr("t", 3, base = "m"), pr("b", 1), pr("m", 2, base = "b"))
        val confirm = PullRequestStack.stackConfirm(all[2], all)!!
        assertEquals(DomainContract.diffUiMergeStack, confirm.title)
        assertEquals("Lands 3 pull requests, bottom-up: #1, #2, #3.", confirm.body)
        assertEquals("t", confirm.throughId)
    }

    @Test
    fun `merged members drop out of the open chain`() {
        val all = listOf(pr("b", 1, state = DomainContract.prStateMerged), pr("m", 2, base = "b"), pr("t", 3, base = "m"))
        val confirm = PullRequestStack.stackConfirm(all[1], all)!!
        assertEquals("Lands 2 pull requests, bottom-up: #2, #3.", confirm.body)
    }

    @Test
    fun `a fork has no stack`() {
        val all = listOf(pr("b", 1), pr("x", 2, base = "b"), pr("y", 3, base = "b"))
        assertNull(PullRequestStack.stackConfirm(all[0], all))
        assertNull(PullRequestStack.stackConfirm(all[1], all))
    }

    @Test
    fun `merge through a member lands it and everything beneath it`() {
        val all = listOf(pr("t", 3, base = "m"), pr("b", 1), pr("m", 2, base = "b"))
        val confirm = PullRequestStack.stackConfirm(all[0], all, through = all[2])!!
        assertEquals(DomainContract.diffUiMergeThrough, confirm.title)
        assertEquals("Lands 2 pull requests, bottom-up: #1, #2. #3 stays open.", confirm.body)
        assertEquals("m", confirm.throughId)
    }

    @Test
    fun `a base branch equal to another head is the stack edge`() {
        val lower = PullRequestEntity(id = "b", number = 1, repositoryId = "r", head = "exp/EXP-1", base = "master")
        val upper = PullRequestEntity(id = "t", number = 2, repositoryId = "r", head = "exp/EXP-2", base = "exp/EXP-1")
        val other = PullRequestEntity(id = "x", number = 3, repositoryId = "q", head = "exp/EXP-3", base = "exp/EXP-1")
        val all = listOf(upper, lower, other)
        assertEquals(listOf("b", "t"), PullRequestStack.openChain(upper, all)!!.map { it.id })
    }

    @Test
    fun `the stack view runs top first, named by the lowest linked identifier`() {
        val all = listOf(pr("t", 3, base = "m"), pr("b", 1), pr("m", 2, base = "b").copy(title = "Own title"))
        val issues = listOf(
            issue("i2", "APP-12", "Second"),
            issue("i1", "APP-11", "First"),
        )
        val links = listOf(
            PullRequestIssueEntity(id = "l1", pullRequestId = "t", issueId = "i2"),
            PullRequestIssueEntity(id = "l2", pullRequestId = "t", issueId = "i1"),
        )
        val view = PullRequestStack.stackView(all[2], all, links, issues)!!
        assertEquals(listOf("t", "m", "b"), view.rows.map { it.pullRequestId })
        assertEquals(listOf("APP-11", "#2", "#1"), view.rows.map { it.identifier })
        assertEquals(listOf("First", "Own title", ""), view.rows.map { it.title })
        assertEquals(listOf(false, true, false), view.rows.map { it.isCurrent })
        assertEquals("main", view.baseBranch)
        assertNull(PullRequestStack.stackView(pr("a", 1), listOf(pr("a", 1)), emptyList(), emptyList()))
    }
}
