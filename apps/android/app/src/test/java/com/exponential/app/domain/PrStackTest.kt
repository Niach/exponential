package com.exponential.app.domain

import com.exponential.app.data.db.IssueEntity
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

// EXP-897 — the PR stack's three rules, the same three tests web
// (`pr-stack.test.ts`), iOS (`PrStackTests`) and the desktop
// (`nest_review_entries_*`) run.
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

    private fun shape(rows: List<PrStack.Nested<IssueEntity>>) =
        rows.map { "${it.entry.id}@${it.depth}${if (it.hasChildren) "+" else ""}" }

    @Test
    fun numbersAMemberFromTheBottomOfTheChain() {
        val bottom = issue("a", branch = "exp/A")
        val middle = issue("b", branch = "exp/B", base = "exp/A")
        val top = issue("c", branch = "exp/C", base = "exp/B")
        val issues = listOf(top, bottom, middle)

        val position = PrStack.stackPosition(middle, issues)!!
        assertEquals(2, position.position)
        assertEquals(3, position.size)
        assertEquals("a", position.below?.id)
        assertEquals("c", position.above?.id)

        assertEquals(listOf("a", "b", "c"), PrStack.stackChain(top, issues).map { it.id })
        // A pull request nobody builds on and that builds on nobody.
        assertNull(PrStack.stackPosition(issue("lone", branch = "exp/L"), issues))
    }

    @Test
    fun stopsAtABaseNobodyInTheListOwns() {
        // `master` is nobody's branch, so `a` is the bottom of the stack.
        val bottom = issue("a", branch = "exp/A", base = "master")
        val top = issue("b", branch = "exp/B", base = "exp/A")
        val issues = listOf(bottom, top)

        assertEquals(listOf("a", "b"), PrStack.stackChain(top, issues).map { it.id })
        assertEquals(1, PrStack.stackPosition(bottom, issues)!!.position)
        assertEquals(listOf("a@0+", "b@1"), shape(PrStack.nestPrStacks(issues)))
    }

    @Test
    fun breaksACycleWhereItFirstAppears() {
        val a = issue("a", branch = "exp/A", base = "exp/B")
        val b = issue("b", branch = "exp/B", base = "exp/A")
        val issues = listOf(a, b)

        // Two members, each named once, however the walk entered the loop.
        assertEquals(setOf("a", "b"), PrStack.stackChain(a, issues).map { it.id }.toSet())
        assertEquals(2, PrStack.stackChain(a, issues).size)
        assertEquals(listOf("a@0+", "b@1"), shape(PrStack.nestPrStacks(issues)))
    }

    @Test
    fun keepsTheCallersRootOrderAndFollowsAParentWithItsChildren() {
        val rows = listOf(
            issue("z", branch = "exp/Z"),
            issue("top", branch = "exp/T", base = "exp/M"),
            issue("bottom", branch = "exp/B"),
            issue("middle", branch = "exp/M", base = "exp/B"),
        )
        assertEquals(
            listOf("z@0", "bottom@0+", "middle@1+", "top@2"),
            shape(PrStack.nestPrStacks(rows)),
        )
    }
}
