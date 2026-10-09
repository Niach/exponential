package com.exponential.app.domain

import com.exponential.app.data.db.BoardEntity
import com.exponential.app.data.db.IssueEntity
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

// EXP-897 / SLOP-3: the PR stack chain (client only), mirrored by web
// (`pr-stack.test.ts`), iOS (`PrStackTests`) and the desktop (`pr_stack`).
class PrStackTest {

    private fun issue(
        id: String,
        identifier: String = id.uppercase(),
        branch: String? = null,
        base: String? = null,
        boardId: String = "board-1",
        prState: String? = null,
    ) = IssueEntity(
        id = id,
        boardId = boardId,
        number = 1,
        identifier = identifier,
        title = "Issue $identifier",
        status = "backlog",
        priority = "none",
        sortOrder = 1.0,
        branch = branch,
        prBaseBranch = base,
        prState = prState,
        createdAt = "2026-09-10T10:00:00Z",
        updatedAt = "2026-09-10T10:00:00Z",
    )

    private fun board(id: String, teamId: String) = BoardEntity(
        id = id,
        teamId = teamId,
        name = id,
        slug = id,
        prefix = "APP",
        color = "#000000",
        sortOrder = 1.0,
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

    // Two teams with an `APP` board each: identifiers and so branch names
    // repeat. Team A's plain pull request must not read team B's stack.
    @Test
    fun aStackIsReadFromTheSubjectsTeamOnly() {
        val boards = listOf(board("a-board", "team-a"), board("a-other", "team-a"), board("b-board", "team-b"))
        val mine = issue("a20", "APP-20", branch = "exp/APP-20", boardId = "a-board", prState = "open")
        val sibling = issue("a7", "APP-7", branch = "exp/APP-7", boardId = "a-other", prState = "open")
        val foreignBottom = issue("b20", "APP-20", branch = "exp/APP-20", boardId = "b-board", prState = "open")
        val foreignTop = issue(
            "b21", "APP-21", branch = "exp/APP-21", base = "exp/APP-20", boardId = "b-board", prState = "open",
        )
        val all = listOf(foreignTop, foreignBottom, sibling, mine)

        // The bug: the account-wide pool chains team B's top onto team A's PR.
        assertEquals("b21", PrStack.stackMergeConfirm(mine, all, PrStack.StackConfirmMode.Stack)?.issueId)

        val pool = PrStack.teamIssues(mine, all, boards)
        assertEquals(setOf("a20", "a7"), pool.map { it.id }.toSet())
        assertNull(PrStack.stackMergeConfirm(mine, pool, PrStack.StackConfirmMode.Stack))
        // Team B still reads its own stack.
        assertEquals(
            listOf("APP-20", "APP-21"),
            PrStack.stackMergeConfirm(
                foreignBottom, PrStack.teamIssues(foreignBottom, all, boards), PrStack.StackConfirmMode.Stack,
            )?.landing,
        )
    }

    @Test
    fun anUnresolvedTeamNeverWidensThePool() {
        val boards = listOf(board("b-board", "team-b"))
        val mine = issue("a20", "APP-20", boardId = "a-board")
        val foreign = issue("b20", "APP-20", boardId = "b-board")
        val all = listOf(foreign, mine)

        // The issue's board has not synced: its own board only.
        assertEquals(listOf("a20"), PrStack.teamIssues(mine, all, boards).map { it.id })
        // A bare run names its team; no team = no pool.
        assertEquals(listOf("b20"), PrStack.teamIssues("team-b", all, boards).map { it.id })
        assertEquals(emptyList<IssueEntity>(), PrStack.teamIssues(null as String?, all, boards))
    }
}
