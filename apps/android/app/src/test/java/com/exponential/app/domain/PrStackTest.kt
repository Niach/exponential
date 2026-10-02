package com.exponential.app.domain

import com.exponential.app.data.db.IssueEntity
import org.junit.Assert.assertEquals
import org.junit.Test

// EXP-897 / SLOP-3: the PR stack chain (client only), mirrored by web
// (`pr-stack.test.ts`), iOS (`PrStackTests`) and the desktop (`pr_stack`).
class PrStackTest {

    private fun issue(
        id: String,
        identifier: String = id.uppercase(),
        branch: String? = null,
        base: String? = null,
    ) = IssueEntity(
        id = id,
        boardId = "board-1",
        number = 1,
        identifier = identifier,
        title = "Issue $identifier",
        status = "backlog",
        priority = "none",
        sortOrder = 1.0,
        branch = branch,
        prBaseBranch = base,
        createdAt = "2026-09-10T10:00:00Z",
        updatedAt = "2026-09-10T10:00:00Z",
    )

    @Test
    fun ordersTheChainFromTheBottom() {
        val bottom = issue("a", branch = "exp/A")
        val middle = issue("b", branch = "exp/B", base = "exp/A")
        val top = issue("c", branch = "exp/C", base = "exp/B")
        val issues = listOf(top, bottom, middle)

        assertEquals(listOf("a", "b", "c"), PrStack.stackChain(middle, issues).map { it.id })
        assertEquals(listOf("a", "b", "c"), PrStack.stackChain(top, issues).map { it.id })
        // A pull request nobody builds on and that builds on nobody.
        val lone = issue("lone", branch = "exp/L")
        assertEquals(listOf("lone"), PrStack.stackChain(lone, issues + lone).map { it.id })
    }

    @Test
    fun stopsAtABaseNobodyInTheListOwns() {
        // `master` is nobody's branch, so `a` is the bottom of the stack.
        val bottom = issue("a", branch = "exp/A", base = "master")
        val top = issue("b", branch = "exp/B", base = "exp/A")
        val issues = listOf(bottom, top)

        assertEquals(listOf("a", "b"), PrStack.stackChain(top, issues).map { it.id })
        assertEquals(listOf("a", "b"), PrStack.stackChain(bottom, issues).map { it.id })
    }

    @Test
    fun breaksACycleWhereItFirstAppears() {
        val a = issue("a", branch = "exp/A", base = "exp/B")
        val b = issue("b", branch = "exp/B", base = "exp/A")
        val issues = listOf(a, b)

        // Two members, each named once, however the walk entered the loop.
        assertEquals(setOf("a", "b"), PrStack.stackChain(a, issues).map { it.id }.toSet())
        assertEquals(2, PrStack.stackChain(a, issues).size)
    }
}
