package com.exponential.app.domain

import com.exponential.app.data.db.CodingSessionEntity
import com.exponential.app.data.db.IssueEntity
import com.exponential.app.ui.session.sessionRowTitle
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

    private fun session(
        id: String,
        parent: String? = null,
        issueId: String? = null,
        // EXP-876: what makes a run a BATCH, and what names it.
        batchIssueIds: String? = null,
        branch: String? = null,
        actionName: String? = null,
    ) =
        CodingSessionEntity(
            id = id,
            issueId = issueId,
            teamId = "team-1",
            userId = "user-1",
            branch = branch,
            batchIssueIds = batchIssueIds,
            actionName = actionName,
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

    // EXP-876: the pill and its sheet are the surface built to name work that
    // spans several issues — and a batch RUN, which spans them, resolved
    // nothing at all before this (it links no issue and stamps no pr_url).
    // Mirrored ×4.
    @Test
    fun reportsABatchBadgeForABatchRunBeforeItsPr() {
        val one = issue("one")
        val two = issue("two")
        val run = session("run", batchIssueIds = """["one","two"]""")
        val built = graph(null, listOf(one, two), session = run, sessions = listOf(run))

        assertEquals(PrGraph.BadgeKind.BATCH, PrGraph.badgeKind(built))
        // The composer's order, so the sheet reads like the row that named it.
        assertEquals(listOf("one", "two"), built.batch?.issues?.map { it.id })
        // No pull request yet: a batch of two is not a stack of two.
        assertEquals(1, built.stack.size)
    }

    @Test
    fun reportsABatchRunsPullRequestOnceItOpens() {
        // The GROUPED entry wins the moment the PR exists: it carries the
        // branch and the base the stack chains on, so a stacked batch still
        // reads `stack+batch` from its run.
        val url = "https://github.com/o/r/pull/9"
        val lower = issue("lower", branch = "exp/LOWER")
        val one = issue("one", branch = "exp/batch-abcd1234", base = "exp/LOWER", prUrl = url)
        val two = issue("two", branch = "exp/batch-abcd1234", base = "exp/LOWER", prUrl = url)
        val run = session(
            "run",
            batchIssueIds = """["one","two"]""",
            branch = "exp/batch-abcd1234",
        )
        val built = graph(null, listOf(lower, one, two), session = run, sessions = listOf(run))

        assertEquals(PrGraph.BadgeKind.STACK_AND_BATCH, PrGraph.badgeKind(built))
        assertEquals(2, built.batch?.issues?.size)
    }

    @Test
    fun reportsNothingForABatchRunWhoseIssuesAreUnknown() {
        // No stored ids and no PR: the row reads "Batch run" and wears no
        // pill, rather than a pill that could say nothing.
        val bare = session("run")
        val built = graph(null, emptyList(), session = bare, sessions = listOf(bare))
        assertNull(PrGraph.badgeKind(built))
        assertNull(built.batch)

        // An ACTION run is never a batch, whatever else it carries.
        val action = session("chat", actionName = "Chat", batchIssueIds = """["one"]""")
        val actionGraph =
            graph(null, listOf(issue("one")), session = action, sessions = listOf(action))
        assertNull(actionGraph.batch)
    }

    // EXP-930: what the OVERLAY draws on a batch's Run face — the covered
    // issues above the run tree, and every tree row named off the synced
    // rows. The sheet used to join no issue at all, so a batch whose issues
    // were already in the store still read `Issue syncing…`.
    @Test
    fun namesABatchRunsIssuesAndItsRunRows() {
        val one = issue("one")
        val two = issue("two")
        val pool = listOf(one, two)
        val batch = session("run", batchIssueIds = """["one","two"]""")
        val child = session("child", parent = "run", issueId = "two")
        val built = graph(null, pool, session = batch, sessions = listOf(batch, child))

        // The `Issues` section the sheet now draws above `Runs`.
        assertEquals(listOf("ONE", "TWO"), built.batch?.issues?.map { it.identifier })

        // EXP-968: the graph NAMES its own rows — the sheet renders these.
        assertEquals(listOf("Issue ONE", "Issue TWO"), built.tree.map { it.title })
        assertEquals(listOf(null, "two"), built.tree.map { it.issue?.id })
        // The bug itself: no joined issue, no name. ONE string ×4.
        assertEquals(ISSUE_SYNCING_TITLE, sessionRowTitle(child, null))
        assertEquals("Issue syncing…", ISSUE_SYNCING_TITLE)
    }

    // EXP-968: the Runs list of the stack overlay said "Issue not synced yet"
    // about every row, because the labels were joined against a SECOND issue
    // snapshot. A run tree whose issues are all synced names every row.
    @Test
    fun namesEveryRunRowFromTheGraphsOwnIssues() {
        val one = issue("one")
        val two = issue("two")
        val root = session("root", issueId = "one")
        val child = session("child", parent = "root", issueId = "two")
        val orphan = session("orphan", parent = "root", issueId = "missing")
        val built = graph(
            one,
            listOf(one, two),
            session = root,
            sessions = listOf(root, child, orphan),
        )

        assertEquals(listOf("root", "child", "orphan"), built.tree.map { it.session.id })
        assertEquals(
            listOf("Issue ONE", "Issue TWO", ISSUE_SYNCING_TITLE),
            built.tree.map { it.title },
        )
        // Only the row whose issue is genuinely missing waits on the sync.
        assertEquals(listOf("one", "two", null), built.tree.map { it.issue?.id })
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
