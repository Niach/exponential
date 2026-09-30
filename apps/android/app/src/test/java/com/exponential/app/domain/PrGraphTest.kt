package com.exponential.app.domain

import com.exponential.app.data.db.CodingSessionEntity
import com.exponential.app.data.db.IssueEntity
import com.exponential.app.data.db.IssueRelationEntity
import com.exponential.app.domain.PrGraph.OverlaySection
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
        status: String = "backlog",
    ) = IssueEntity(
        id = id,
        boardId = "board-1",
        number = 1,
        identifier = identifier,
        title = "Issue $identifier",
        status = status,
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
        relations: List<IssueRelationEntity> = emptyList(),
    ) = PrGraph.build(issue, session, issues, sessions, relations)

    private fun blocks(blocker: String, blocked: String) = IssueRelationEntity(
        id = "$blocker-blocks-$blocked",
        issueId = blocker,
        relatedIssueId = blocked,
        type = "blocks",
        source = "user",
        teamId = "team-1",
        createdAt = "2026-09-10T10:00:00Z",
        updatedAt = "2026-09-10T10:00:00Z",
    )

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
    fun `names the representative issue and the count on the stacked chip`() {
        val url = "https://github.com/acme/app/pull/9"
        val lower = issue("lower", branch = "exp/LOWER")
        val one = issue("one", branch = "exp/batch-abcd1234", base = "exp/LOWER", prUrl = url)
        val two = issue("two", branch = "exp/batch-abcd1234", base = "exp/LOWER", prUrl = url)
        // A batch inside a stack: the subject PR's representative, every other
        // issue on the stack behind it.
        val both = graph(two, listOf(lower, one, two))
        assertEquals("one", PrGraph.badgeChip(both)?.issue?.id)
        assertEquals(2, PrGraph.badgeChip(both)?.count)
        // A plain batch: the others of the batch.
        val batch = graph(one, listOf(one, two.copy(prBaseBranch = null)))
        assertEquals(1, PrGraph.badgeChip(batch)?.count)
        // SLOP-16 r5: a run family alone earns no chip.
        val sessions = listOf(session("child", parent = "root"), session("root"))
        val family = graph(null, emptyList(), session = sessions[0], sessions = sessions)
        assertNull(PrGraph.badgeChip(family))
        // No badge = no chip.
        val lone = issue("lone")
        assertNull(PrGraph.badgeChip(graph(lone, listOf(lone))))
    }

    // SLOP-16 r5: a run family alone earns NO badge; a PR relation still does.
    @Test
    fun `a run family alone earns no badge`() {
        val sessions = listOf(session("child", parent = "root"), session("root"))
        val family = graph(null, emptyList(), session = sessions[0], sessions = sessions)
        assertNull(PrGraph.badgeShape(family))
        val alone = session("alone")
        assertNull(PrGraph.badgeShape(graph(null, emptyList(), session = alone, sessions = listOf(alone))))
        val url = "https://github.com/acme/app/pull/9"
        val batchA = issue("bata", prUrl = url)
        val batchB = issue("batb", prUrl = url)
        val batched = graph(batchA, listOf(batchA, batchB), session = sessions[0], sessions = sessions)
        assertEquals(PrGraph.BadgeShape.BATCH, PrGraph.badgeShape(batched))
    }

    // EXP-1097: open blockers alone earn the chip, behind the PR relations;
    // the front chip is the first open blocker.
    @Test
    fun `shapes a blocked badge for an issue with open blockers`() {
        val me = issue("me")
        val b1 = issue("b1")
        val b2 = issue("b2")
        val closed = issue("closed", status = "done")
        val relations = listOf(blocks("b2", "me"), blocks("b1", "me"), blocks("closed", "me"))
        val blocked = graph(me, listOf(me, b1, b2, closed), relations = relations)
        assertEquals(PrGraph.BadgeShape.BLOCKED, PrGraph.badgeShape(blocked))
        assertEquals("b1", PrGraph.badgeChip(blocked)?.issue?.id)
        assertEquals(1, PrGraph.badgeChip(blocked)?.count)
        // Only closed blockers: no chip.
        assertNull(PrGraph.badgeShape(graph(me, listOf(me, closed), relations = relations)))
        // SLOP-16 r5: a run family no longer wins over the blockers.
        val runs = listOf(session("r1", issueId = "me"), session("r2", parent = "r1"))
        assertEquals(
            PrGraph.BadgeShape.BLOCKED,
            PrGraph.badgeShape(
                graph(me, listOf(me, b1), session = runs[0], sessions = runs, relations = relations),
            ),
        )
        // A batch wins over both.
        val url = "https://github.com/acme/app/pull/9"
        val batchA = issue("me", prUrl = url)
        val batchB = issue("batb", prUrl = url)
        assertEquals(
            PrGraph.BadgeShape.BATCH,
            PrGraph.badgeShape(
                graph(
                    batchA,
                    listOf(batchA, batchB, b1),
                    session = runs[0],
                    sessions = runs,
                    relations = relations,
                ),
            ),
        )
    }

    // SLOP-16 r5: the relations card's bands in ONE fixed order, each only
    // when it has rows; runs are never a section.
    @Test
    fun `lists the overlay's bands in one order`() {
        val url = "https://github.com/acme/app/pull/9"
        val me = issue("me", prUrl = url)
        val partner = issue("partner", prUrl = url)
        val blocker = issue("blocker")
        val runs = listOf(session("r1", issueId = "me"), session("r2", parent = "r1"))
        val built = graph(
            me,
            listOf(me, partner, blocker),
            session = runs[0],
            sessions = runs,
            relations = listOf(blocks("blocker", "me")),
        )
        assertEquals(listOf(OverlaySection.BLOCKED, OverlaySection.BATCH), PrGraph.overlaySections(built))
        assertEquals(listOf("partner"), PrGraph.batchPartners(built).map { it.id })
        val lone = issue("lone")
        assertEquals(emptyList<OverlaySection>(), PrGraph.overlaySections(graph(lone, listOf(lone))))
        // A run family alone lists nothing.
        val family = graph(null, emptyList(), session = runs[1], sessions = runs)
        assertEquals(emptyList<OverlaySection>(), PrGraph.overlaySections(family))
    }

    // SLOP-16 r5: the stack band = the OTHER pull requests, bottom-up.
    @Test
    fun `lists the other stack members bottom-up without the subject`() {
        val bottom = issue("a", branch = "exp/A")
        val middle = issue("b", branch = "exp/B", base = "exp/A")
        val top = issue("c", branch = "exp/C", base = "exp/B")
        val built = graph(middle, listOf(bottom, middle, top))
        assertEquals(listOf("a", "c"), PrGraph.otherStackEntries(built).map { it.entry.representative.id })
        assertEquals(listOf(OverlaySection.STACK), PrGraph.overlaySections(built))
        val lone = issue("lone", branch = "exp/L")
        assertTrue(PrGraph.otherStackEntries(graph(lone, listOf(lone))).isEmpty())
    }

    // SLOP-16 r5: a bare batch run has no subject issue — every covered issue
    // is a partner.
    @Test
    fun `a bare batch run lists its whole batch`() {
        val url = "https://github.com/acme/app/pull/9"
        val one = issue("one", prUrl = url)
        val two = issue("two", prUrl = url)
        val run = session("run", batchIssueIds = "[\"one\",\"two\"]").copy(prUrl = url)
        val built = graph(null, listOf(one, two), session = run, sessions = listOf(run))
        assertEquals(listOf("one", "two"), PrGraph.batchPartners(built).map { it.id })
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

    // SLOP-16 r5: THE "Related work" sheet's words, byte-identical ×4 (web
    // `PR_GRAPH_OVERLAY_COPY`, iOS `PrGraphBadge.swift`, desktop `pr_graph.rs`).
    @Test
    fun `pins the related work view's copy`() {
        assertEquals("Related work", PrGraph.OverlayCopy.RELATED_WORK_TITLE)
        assertEquals("Blocked by", PrGraph.OverlayCopy.BLOCKED)
        assertEquals("Same pull request", PrGraph.OverlayCopy.BATCH)
        assertEquals("Pull request stack", PrGraph.OverlayCopy.STACK)
        assertEquals("Nothing else is linked to this issue.", PrGraph.OverlayCopy.EMPTY)
    }
}
