package com.exponential.app.domain

import com.exponential.app.data.db.CodingSessionEntity
import com.exponential.app.data.db.IssueEntity
import com.exponential.app.data.db.WorkflowEntity
import com.exponential.app.data.db.WorkflowNodeEntity
import org.junit.Assert.assertEquals
import org.junit.Ignore
import org.junit.Test

// EXP-818 — the session tree's four rules, the same four tests web
// (`session-tree.test.ts`), iOS (`SessionTreeTests`) and the desktop
// (`nest_sessions_*`) run.
class SessionTreeTest {
    private data class Row(val id: String, val parent: String?, val startedAt: String = "2026-09-10T10:00:00Z")

    private fun nest(rows: List<Row>) = SessionTree.nest(rows, { it.id }, { it.parent }, { it.startedAt })

    private fun shape(rows: List<SessionTree.Row<Row>>) =
        rows.map { "${it.session.id}@${it.depth}${if (it.hasChildren) "+" else ""}" }

    @Test
    fun keepsTheCallersRootOrder() {
        assertEquals(listOf("b@0", "a@0", "c@0"), shape(nest(listOf(Row("b", null), Row("a", null), Row("c", null)))))
    }

    @Test
    fun nestsAChildOnlyUnderAParentThatIsListed() {
        assertEquals(
            listOf("p@0+", "c@1", "orphan@0"),
            shape(nest(listOf(Row("p", null), Row("c", "p"), Row("orphan", "gone")))),
        )
    }

    @Test
    fun listsChildrenRightAfterTheirParentOldestFirstRecursively() {
        val rows = nest(
            listOf(
                Row("late", "p", "2026-09-10T12:00:00Z"),
                Row("p", null),
                Row("grand", "early", "2026-09-10T13:00:00Z"),
                Row("early", "p", "2026-09-10T11:00:00Z"),
                Row("z", null),
            ),
        )
        assertEquals(listOf("p@0+", "early@1+", "grand@2", "late@1", "z@0"), shape(rows))
        assertEquals(listOf("early", "grand", "late"), SessionTree.descendantIds(rows, "p") { it.id })
        assertEquals(listOf("grand"), SessionTree.descendantIds(rows, "early") { it.id })
        assertEquals(emptyList<String>(), SessionTree.descendantIds(rows, "z") { it.id })
    }

    // EXP-897: the fold — a collapsed parent hides its whole subtree, itself
    // excepted. Same test name ×4.
    @Test
    fun hidesRowsUnderACollapsedParent() {
        val rows = nest(
            listOf(
                Row("p", null),
                Row("c", "p", "2026-09-10T11:00:00Z"),
                Row("g", "c", "2026-09-10T12:00:00Z"),
                Row("z", null),
            ),
        )
        assertEquals(listOf("p@0+", "c@1+", "g@2", "z@0"), shape(rows))
        assertEquals(
            listOf("p@0+", "z@0"),
            shape(SessionTree.visibleRows(rows, setOf("p")) { it.id }),
        )
        assertEquals(
            listOf("p@0+", "c@1+", "z@0"),
            shape(SessionTree.visibleRows(rows, setOf("c")) { it.id }),
        )
        assertEquals(shape(rows), shape(SessionTree.visibleRows(rows, emptySet()) { it.id }))
    }

    @Test
    fun breaksACycleWhereItFirstAppears() {
        assertEquals(
            listOf("self@0", "a@0+", "b@1"),
            shape(nest(listOf(Row("a", "b"), Row("b", "a"), Row("self", "self")))),
        )
    }
}

// EXP-1050 — the NODE-based tree (`sessionTree`, EXP-996 implements the
// EXP-1029 contract). The web table's 14 cases, by their `it(...)` names, so
// the ×4 lockstep reads straight across (web `session-tree.test.ts`, desktop
// `domain::session_tree`, iOS `SessionTreeNodeTests`).
class SessionTreeNodeTest {
    private fun row(
        id: String,
        at: String = "2026-09-01T10:00:00Z",
        resumedFromId: String? = null,
        parentSessionId: String? = null,
        issueId: String? = null,
        batchIssueIds: String? = null,
        startedReason: String? = null,
        workflowId: String? = null,
        workflowNodeId: String? = null,
        workflowRole: String? = null,
    ) = CodingSessionEntity(
        id = id,
        teamId = "team-1",
        userId = "user-1",
        issueId = issueId,
        resumedFromId = resumedFromId,
        parentSessionId = parentSessionId,
        batchIssueIds = batchIssueIds,
        startedReason = startedReason,
        workflowId = workflowId,
        workflowNodeId = workflowNodeId,
        workflowRole = workflowRole,
        startedAt = at,
        createdAt = at,
        updatedAt = at,
    )

    private fun issue(id: String, identifier: String, branch: String?, base: String?) = IssueEntity(
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
        createdAt = "2026-09-01T10:00:00Z",
        updatedAt = "2026-09-01T10:00:00Z",
    )

    private fun workflow(id: String, name: String) =
        WorkflowEntity(id = id, teamId = "team-1", name = name, status = "running")

    private fun workflowNode(workflowId: String, issueId: String, sessionId: String? = null) =
        WorkflowNodeEntity(id = "wn-$issueId", workflowId = workflowId, issueId = issueId, sessionId = sessionId)

    /** The tree as `key(child,child)` strings — the web table's `ids` helper. */
    private fun shape(nodes: List<SessionTreeNode>): List<String> = nodes.map { node ->
        val key = sessionTreeNodeKey(node)
        if (node.children.isEmpty()) key else "$key(${shape(node.children).joinToString(",")})"
    }

    private fun epochMs(iso: String): Long = WireTimestamps.parseEpochMs(iso)!!

    @Test
    fun `walks groups and children depth-first, sessions only`() {
        val leaf = SessionTreeNode.Session(session = row("b"), chain = listOf(row("b")))
        val tree = listOf(
            SessionTreeNode.Workflow(workflowId = "w", name = "W", children = listOf(leaf)),
            SessionTreeNode.Session(session = row("a"), chain = listOf(row("a"))),
        )
        assertEquals(listOf("b", "a"), flattenSessionTree(tree).map { it.session.id })
    }

    @Test
    fun `lists unrelated sessions at top level, newest activity first`() {
        val a = row("a", "2026-09-01T10:00:00Z")
        val b = row("b", "2026-09-01T11:00:00Z")
        assertEquals(listOf("b", "a"), shape(sessionTree(listOf(a, b))))
    }

    @Test
    fun `nests a child under its parentSessionId`() {
        val parent = row("p")
        val child = row("c", "2026-09-01T10:30:00Z", parentSessionId = "p")
        assertEquals(listOf("p(c)"), shape(sessionTree(listOf(child, parent))))
    }

    @Test
    fun `collapses a resume succession into one node keyed by its newest row`() {
        val first = row("r1", "2026-09-01T10:00:00Z")
        val second = row("r2", "2026-09-01T12:00:00Z", resumedFromId = "r1")
        val tree = sessionTree(listOf(first, second))
        assertEquals(listOf("r2"), shape(tree))
        assertEquals(
            listOf("r1", "r2"),
            (tree[0] as SessionTreeNode.Session).chain.map { it.id },
        )
    }

    @Test
    fun `follows a child to its parent's resume succession`() {
        val p1 = row("p1")
        val p2 = row("p2", "2026-09-01T11:00:00Z", resumedFromId = "p1")
        val child = row("c", parentSessionId = "p1")
        assertEquals(listOf("p2(c)"), shape(sessionTree(listOf(p1, p2, child))))
    }

    @Test
    fun `groups the sessions of one workflow under a workflow node`() {
        val n1 = row("n1", issueId = "i1", startedReason = "workflow")
        val n2 = row("n2", "2026-09-01T11:00:00Z", issueId = "i2", startedReason = "workflow")
        val tree = sessionTree(
            listOf(n1, n2),
            SessionTreeContext(
                workflows = listOf(workflow("w", "EXP-996 +5")),
                workflowNodes = listOf(workflowNode("w", "i1"), workflowNode("w", "i2")),
            ),
        )
        assertEquals(listOf("workflow:w(n2,n1)"), shape(tree))
        assertEquals("EXP-996 +5", (tree[0] as SessionTreeNode.Workflow).name)
    }

    // The rule the group row's NAME comes from: a `workflow` run whose workflow
    // the caller has not synced stays a plain row (web's `workflows` guard).
    @Test
    fun `leaves a workflow run ungrouped when the workflow is not listed`() {
        val n1 = row("n1", issueId = "i1", startedReason = "workflow")
        val n2 = row("n2", "2026-09-01T11:00:00Z", issueId = "i2", startedReason = "workflow")
        val tree = sessionTree(
            listOf(n1, n2),
            SessionTreeContext(workflowNodes = listOf(workflowNode("w", "i1"), workflowNode("w", "i2"))),
        )
        assertEquals(listOf("n2", "n1"), shape(tree))
    }

    // EXP-978: a batch node run names its issues in `batch_issue_ids`, not in
    // `issue_id` — the covered set is what resolves its workflow.
    @Test
    fun `groups a batch node run by the issues it covers`() {
        val batch = row("batch", batchIssueIds = """["i2","i1"]""")
        val plain = row("n1", "2026-09-01T11:00:00Z", issueId = "i1")
        val tree = sessionTree(
            listOf(batch, plain),
            SessionTreeContext(
                workflows = listOf(workflow("w", "W")),
                workflowNodes = listOf(workflowNode("w", "i1"), workflowNode("w", "i2")),
            ),
        )
        assertEquals(listOf("workflow:w(n1,batch)"), shape(tree))
    }

    @Test
    fun `groups a stack under its lowest issue, in linear order`() {
        val low = row("s-low", issueId = "i-low")
        val mid = row("s-mid", "2026-09-01T11:00:00Z", issueId = "i-mid")
        val top = row("s-top", "2026-09-01T12:00:00Z", issueId = "i-top")
        val tree = sessionTree(
            listOf(top, low, mid),
            SessionTreeContext(
                issues = listOf(
                    issue("i-low", "APP-1", "exp/APP-1", null),
                    issue("i-mid", "APP-2", "exp/APP-2", "exp/APP-1"),
                    issue("i-top", "APP-3", "exp/APP-3", "exp/APP-2"),
                ),
            ),
        )
        assertEquals(listOf("stack:i-low(s-low,s-mid,s-top)"), shape(tree))
    }

    // An issue-less run is in no stack, but it is still a ROW — dropping it
    // would hide every chat, action and batch run the moment issues sync.
    @Test
    fun `keeps an issue-less run beside a stack group`() {
        val chat = row("chat", "2026-09-01T09:00:00Z")
        val low = row("s-low", issueId = "i-low")
        val top = row("s-top", "2026-09-01T11:00:00Z", issueId = "i-top")
        val tree = sessionTree(
            listOf(chat, low, top),
            SessionTreeContext(
                issues = listOf(
                    issue("i-low", "APP-1", "exp/APP-1", null),
                    issue("i-top", "APP-2", "exp/APP-2", "exp/APP-1"),
                ),
            ),
        )
        assertEquals(listOf("stack:i-low(s-low,s-top)", "chat"), shape(tree))
    }

    @Test
    fun `sorts groups by their last activity among the top-level nodes`() {
        val lone = row("lone", "2026-09-01T11:30:00Z")
        val n1 = row("n1", "2026-09-01T10:00:00Z", issueId = "i1")
        val n2 = row("n2", "2026-09-01T12:00:00Z", issueId = "i2")
        val tree = sessionTree(
            listOf(lone, n1, n2),
            SessionTreeContext(
                workflows = listOf(workflow("w", "W")),
                workflowNodes = listOf(workflowNode("w", "i1"), workflowNode("w", "i2")),
            ),
        )
        assertEquals(listOf("workflow:w(n2,n1)", "lone"), shape(tree))
        assertEquals(epochMs("2026-09-01T12:00:00Z"), tree[0].lastActivityAt)
    }

    @Test
    fun `puts an orphan child whose parent is gone at top level`() {
        val orphan = row("o", parentSessionId = "gone")
        assertEquals(listOf("o"), shape(sessionTree(listOf(orphan))))
    }

    @Test
    fun `keeps children in creation order under their parent`() {
        val parent = row("p")
        val c1 = row("c1", "2026-09-01T10:10:00Z", parentSessionId = "p")
        val c2 = row("c2", "2026-09-01T10:20:00Z", parentSessionId = "p")
        assertEquals(listOf("p(c1,c2)"), shape(sessionTree(listOf(c2, parent, c1))))
    }

    // ── visibleSessionTreeRows (EXP-996): what the EXP-965 connector is drawn
    //    over — the same flattening on all four clients.
    private fun workflowTree(): List<SessionTreeNode> {
        val parent = row("p", "2026-09-01T11:00:00Z", issueId = "i1")
        val child = row("c", "2026-09-01T10:30:00Z", parentSessionId = "p")
        val other = row("n2", "2026-09-01T10:00:00Z", issueId = "i2")
        return sessionTree(
            listOf(parent, child, other),
            SessionTreeContext(
                workflows = listOf(workflow("w", "W")),
                workflowNodes = listOf(workflowNode("w", "i1"), workflowNode("w", "i2")),
            ),
        )
    }

    @Test
    fun `flattens groups and children with their depths`() {
        assertEquals(
            listOf("workflow:w" to 0, "p" to 1, "c" to 2, "n2" to 1),
            visibleSessionTreeRows(workflowTree()).map { it.key to it.depth },
        )
    }

    @Test
    fun `hides everything under a collapsed node`() {
        val rows = visibleSessionTreeRows(workflowTree(), setOf("p"))
        assertEquals(listOf("workflow:w", "p", "n2"), rows.map { it.key })
        assertEquals(true, rows.first { it.key == "p" }.hasChildren)
    }

    @Test
    fun `folds a whole group away`() {
        assertEquals(
            listOf("workflow:w"),
            visibleSessionTreeRows(workflowTree(), setOf("workflow:w")).map { it.key },
        )
    }

    @Test
    fun `keys a session by its id and a group by its kind`() {
        assertEquals("workflow:w", sessionTreeNodeKey(workflowTree()[0]))
        assertEquals(
            "stack:i-low",
            sessionTreeNodeKey(SessionTreeNode.Stack(rootIssueId = "i-low")),
        )
    }

    // A group is its children: an empty one is not a row at all.
    @Test
    fun `drops a childless group row`() {
        val empty = SessionTreeNode.Workflow(workflowId = "w", name = "W")
        assertEquals(emptyList<String>(), visibleSessionTreeRows(listOf(empty)).map { it.key })
    }

    // ── EXP-1082: workflow membership (`workflow_id` / `workflow_node_id` /
    //    `workflow_role`) — the cases EXP-1068 implements, ×4 same names.

    @Ignore("EXP-1068")
    @Test
    fun `groups by workflow id before any heuristic`() {
        TODO("EXP-1068")
    }

    @Ignore("EXP-1068")
    @Test
    fun `nests a review run under its node's author row`() {
        TODO("EXP-1068")
    }

    @Ignore("EXP-1068")
    @Test
    fun `keeps a switched reviewer under its node`() {
        TODO("EXP-1068")
    }

    @Ignore("EXP-1068")
    @Test
    fun `lists a base merge as a child of the group`() {
        TODO("EXP-1068")
    }

    @Ignore("EXP-1068")
    @Test
    fun `names a plan-only group after the plan`() {
        TODO("EXP-1068")
    }

    @Ignore("EXP-1068")
    @Test
    fun `keeps a foreign chat that resumed a workflow run inside the group`() {
        TODO("EXP-1068")
    }

    @Ignore("EXP-1068")
    @Test
    fun `groups a person's run on a compound node's sub-issue`() {
        TODO("EXP-1068")
    }

    @Ignore("EXP-1068")
    @Test
    fun `flags a node with two live author runs`() {
        TODO("EXP-1068")
    }
}
