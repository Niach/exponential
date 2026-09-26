package com.exponential.app.domain

import com.exponential.app.data.db.CodingSessionEntity
import com.exponential.app.data.db.IssueEntity
import com.exponential.app.data.db.WorkflowEntity
import com.exponential.app.data.db.WorkflowNodeEntity
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
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

// EXP-1050/EXP-1068 — the NODE-based tree (`sessionTree`, the EXP-1029
// contract). The web table's cases by their `it(...)` names, so the ×4
// lockstep reads straight across (web `session-tree.test.ts`, desktop
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
        status: String = "running",
        branch: String? = null,
        member: Member? = null,
    ) = CodingSessionEntity(
        id = id,
        teamId = "team-1",
        userId = "user-1",
        issueId = issueId,
        resumedFromId = resumedFromId,
        parentSessionId = parentSessionId,
        batchIssueIds = batchIssueIds,
        startedReason = startedReason,
        status = status,
        branch = branch,
        workflowId = member?.workflowId,
        workflowNodeId = member?.nodeId,
        workflowRole = member?.role,
        startedAt = at,
        createdAt = at,
        updatedAt = at,
    )

    /** EXP-1082 §1: the server-stamped membership of workflow `w`. */
    data class Member(val nodeId: String?, val role: String? = "author", val workflowId: String = "w")

    private fun member(nodeId: String?, role: String? = "author") = Member(nodeId, role)

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

    private fun workflow(id: String, name: String, status: String = "running") =
        WorkflowEntity(id = id, teamId = "team-1", name = name, status = status)

    private val workflows = listOf(workflow("w", "Checkout rewrite"))

    private fun workflowNode(
        workflowId: String,
        issueId: String,
        sessionId: String? = null,
        id: String = "wn-$issueId",
        state: String = "blocked",
    ) = WorkflowNodeEntity(id = id, workflowId = workflowId, issueId = issueId, sessionId = sessionId, state = state)

    /** The tree as `key(child,child)` strings — the web table's `ids` helper. */
    private fun shape(nodes: List<SessionTreeNode>): List<String> = nodes.map { node ->
        val key = sessionTreeNodeKey(node)
        if (node.children.isEmpty()) key else "$key(${shape(node.children).joinToString(",")})"
    }

    private fun group(tree: List<SessionTreeNode>): SessionTreeNode.Workflow =
        tree.first() as SessionTreeNode.Workflow

    private fun sessionAt(tree: List<SessionTreeNode>, vararg path: Int): SessionTreeNode.Session {
        var cursor: SessionTreeNode = tree[path[0]]
        for (index in path.drop(1)) cursor = cursor.children[index]
        return cursor as SessionTreeNode.Session
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
        // EXP-1068: the rows' own `workflowId` groups them; the
        // `workflow_nodes` rows only feed the caption.
        val n1 = row("n1", issueId = "i1", startedReason = "workflow", member = member("n1"))
        val n2 = row("n2", "2026-09-01T11:00:00Z", issueId = "i2", startedReason = "workflow", member = member("n2"))
        val tree = sessionTree(
            listOf(n1, n2),
            SessionTreeContext(
                workflows = listOf(workflow("w", "EXP-996 +5")),
                workflowNodes = listOf(workflowNode("w", "i1"), workflowNode("w", "i2")),
            ),
        )
        assertEquals(listOf("workflow:w(n2,n1)"), shape(tree))
        assertEquals("EXP-996 +5", group(tree).name)
    }

    // The rule the group row's NAME comes from: a stamped run whose workflow
    // the caller has not synced stays a plain row.
    @Test
    fun `leaves a workflow run ungrouped when the workflow is not listed`() {
        val n1 = row("n1", issueId = "i1", startedReason = "workflow", member = member("n1"))
        val n2 = row("n2", "2026-09-01T11:00:00Z", issueId = "i2", startedReason = "workflow", member = member("n2"))
        val tree = sessionTree(
            listOf(n1, n2),
            SessionTreeContext(workflowNodes = listOf(workflowNode("w", "i1"), workflowNode("w", "i2"))),
        )
        assertEquals(listOf("n2", "n1"), shape(tree))
    }

    // EXP-978: a batch node run (a compound node) is grouped by its stamp,
    // not by the issues it covers.
    @Test
    fun `groups a batch node run by the issues it covers`() {
        val batch = row("batch", batchIssueIds = """["i2","i1"]""", member = member("n2"))
        val plain = row("n1", "2026-09-01T11:00:00Z", issueId = "i1", member = member("n1"))
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
        val n1 = row("n1", "2026-09-01T10:00:00Z", issueId = "i1", member = member("n1"))
        val n2 = row("n2", "2026-09-01T12:00:00Z", issueId = "i2", member = member("n2"))
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
        val parent = row("p", "2026-09-01T11:00:00Z", issueId = "i1", member = member("n1"))
        val child = row("c", "2026-09-01T10:30:00Z", parentSessionId = "p")
        val other = row("n2", "2026-09-01T10:00:00Z", issueId = "i2", member = member("n2"))
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

    // ── EXP-1082 → EXP-1068: membership-first grouping, ×4 same names.

    @Test
    fun `groups by workflow id before any heuristic`() {
        val a = row("a", "2026-09-01T11:00:00Z", issueId = "i9", member = member("n1"))
        val x = row("x", issueId = "i-other")
        assertEquals(
            listOf("workflow:w(a)", "x"),
            shape(sessionTree(listOf(a, x), SessionTreeContext(workflows = workflows))),
        )
    }

    @Test
    fun `never groups an unstamped row, whatever workflow_nodes say`() {
        val n1 = row("n1", issueId = "i1", startedReason = "workflow")
        val tree = sessionTree(
            listOf(n1),
            SessionTreeContext(
                workflows = workflows,
                workflowNodes = listOf(workflowNode("w", "i1", sessionId = "n1")),
            ),
        )
        assertEquals(listOf("n1"), shape(tree))
    }

    @Test
    fun `leaves a stamped run ungrouped when the workflow is not listed`() {
        val a = row("a", issueId = "i1", member = member("n1"))
        assertEquals(listOf("a"), shape(sessionTree(listOf(a), SessionTreeContext())))
    }

    @Test
    fun `nests a review run under its node's author row`() {
        val a = row("a", "2026-09-01T10:00:00Z", issueId = "i1", member = member("n1"))
        val r = row(
            "r", "2026-09-01T11:00:00Z",
            branch = "exp/wf-2f353e88-review-EXP-1068-r1",
            member = member("n1", "review"),
        )
        val b = row("b", "2026-09-01T09:00:00Z", issueId = "i2", member = member("n2"))
        val tree = sessionTree(listOf(a, r, b), SessionTreeContext(workflows = workflows))
        assertEquals(listOf("workflow:w(a(r),b)"), shape(tree))
        assertEquals(1, sessionAt(tree, 0, 0, 0).reviewRound)
        assertEquals(null, sessionAt(tree, 0, 0).reviewRound)
        assertEquals(false, sessionAt(tree, 0, 0).duplicateLive)
    }

    @Test
    fun `keeps a switched reviewer under its node`() {
        val a = row("a", "2026-09-01T10:00:00Z", issueId = "i1", member = member("n1"))
        val r1 = row(
            "r1", "2026-09-01T11:00:00Z", status = "ended",
            branch = "exp/wf-2f353e88-review-EXP-1068-r2",
            member = member("n1", "review"),
        )
        val r2 = row(
            "r2", "2026-09-01T12:00:00Z", resumedFromId = "r1",
            branch = "exp/wf-2f353e88-review-EXP-1068-r2",
            member = member("n1", "review"),
        )
        val tree = sessionTree(listOf(a, r1, r2), SessionTreeContext(workflows = workflows))
        assertEquals(listOf("workflow:w(a(r2))"), shape(tree))
        val reviewer = sessionAt(tree, 0, 0, 0)
        assertEquals(listOf("r1", "r2"), reviewer.chain.map { it.id })
        assertEquals(2, reviewer.reviewRound)
        assertEquals(false, sessionAt(tree, 0, 0).duplicateLive)
    }

    @Test
    fun `nests a review under the live author, not an ended one`() {
        val dead = row("a0", "2026-09-01T12:00:00Z", issueId = "i1", status = "ended", member = member("n1"))
        val live = row("a1", "2026-09-01T10:00:00Z", issueId = "i1", member = member("n1"))
        val r = row("r", "2026-09-01T11:00:00Z", member = member("n1", "review"))
        assertEquals(
            listOf("workflow:w(a0,a1(r))"),
            shape(sessionTree(listOf(dead, live, r), SessionTreeContext(workflows = workflows))),
        )
    }

    @Test
    fun `lists a review whose node has no author as a child of the group`() {
        val r = row("r", member = member("n1", "review"))
        assertEquals(
            listOf("workflow:w(r)"),
            shape(sessionTree(listOf(r), SessionTreeContext(workflows = workflows))),
        )
    }

    @Test
    fun `lists a base merge as a child of the group`() {
        val a = row("a", "2026-09-01T10:00:00Z", issueId = "i1", member = member("n1"))
        val m = row("m", "2026-09-01T11:00:00Z", member = member(null, "base_merge"))
        val p = row("p", "2026-09-01T08:00:00Z", member = member(null, "plan"))
        val rp = row("rp", "2026-09-01T12:00:00Z", member = member(null, "replan"))
        assertEquals(
            listOf("workflow:w(rp,m,a,p)"),
            shape(sessionTree(listOf(a, m, p, rp), SessionTreeContext(workflows = workflows))),
        )
    }

    @Test
    fun `names a plan-only group after the plan`() {
        val tree = sessionTree(
            listOf(row("p", member = member(null, "plan"))),
            SessionTreeContext(workflows = listOf(workflow("w", "Checkout rewrite", status = "draft"))),
        )
        assertEquals(listOf("workflow:w(p)"), shape(tree))
        assertEquals("Checkout rewrite", group(tree).name)
        assertEquals("draft", group(tree).status)
    }

    @Test
    fun `keeps a foreign chat that resumed a workflow run inside the group`() {
        val c = row("c", "2026-09-01T11:00:00Z")
        val a = row("a", "2026-09-01T10:00:00Z", issueId = "i1", member = member("n1"))
        val a2 = row(
            "a2", "2026-09-01T12:00:00Z",
            resumedFromId = "a", parentSessionId = "c", issueId = "i1", member = member("n1"),
        )
        assertEquals(
            listOf("workflow:w(a2)", "c"),
            shape(sessionTree(listOf(c, a, a2), SessionTreeContext(workflows = workflows))),
        )
    }

    @Test
    fun `nests a child of a node run under it inside the group`() {
        val a = row("a", "2026-09-01T10:00:00Z", issueId = "i1", member = member("n1"))
        val kid = row("kid", "2026-09-01T10:30:00Z", parentSessionId = "a", member = member("n1", null))
        val plain = row("plain", "2026-09-01T10:40:00Z", parentSessionId = "a")
        assertEquals(
            listOf("workflow:w(a(kid,plain))"),
            shape(sessionTree(listOf(a, kid, plain), SessionTreeContext(workflows = workflows))),
        )
    }

    @Test
    fun `groups a person's run on a compound node's sub-issue`() {
        val mine = row("mine", "2026-09-01T11:00:00Z", issueId = "i2", member = member("n1"))
        val b = row("b", "2026-09-01T10:00:00Z", issueId = "i3", member = member("n2"))
        val tree = sessionTree(
            listOf(mine, b),
            SessionTreeContext(
                workflows = workflows,
                workflowNodes = listOf(workflowNode("w", "i1", id = "n1", state = "running")),
            ),
        )
        assertEquals(listOf("workflow:w(mine,b)"), shape(tree))
    }

    @Test
    fun `flags a node with two live author runs`() {
        val a1 = row("a1", "2026-09-01T10:00:00Z", issueId = "i1", member = member("n1"))
        val a2 = row("a2", "2026-09-01T11:00:00Z", issueId = "i1", member = member("n1"))
        val tree = sessionTree(listOf(a1, a2), SessionTreeContext(workflows = workflows))
        assertEquals(listOf("workflow:w(a2,a1)"), shape(tree))
        assertEquals(true, sessionAt(tree, 0, 0).duplicateLive)
        assertEquals(true, sessionAt(tree, 0, 1).duplicateLive)
    }

    @Test
    fun `flags a node with two live reviewers, on its author row`() {
        val a = row("a", "2026-09-01T10:00:00Z", issueId = "i1", member = member("n1"))
        val r1 = row("r1", "2026-09-01T11:00:00Z", member = member("n1", "review"))
        val r2 = row("r2", "2026-09-01T11:30:00Z", member = member("n1", "review"))
        val tree = sessionTree(listOf(a, r1, r2), SessionTreeContext(workflows = workflows))
        assertEquals(listOf("workflow:w(a(r1,r2))"), shape(tree))
        assertEquals(true, sessionAt(tree, 0, 0).duplicateLive)
        assertEquals(false, sessionAt(tree, 0, 0, 0).duplicateLive)
    }

    @Test
    fun `does not flag a node whose second author run has ended`() {
        val a1 = row("a1", "2026-09-01T10:00:00Z", issueId = "i1", status = "ended", member = member("n1"))
        val a2 = row("a2", "2026-09-01T11:00:00Z", issueId = "i1", member = member("n1"))
        val tree = sessionTree(listOf(a1, a2), SessionTreeContext(workflows = workflows))
        assertEquals(false, sessionAt(tree, 0, 0).duplicateLive)
    }

    @Test
    fun `counts the group's live runs and landed nodes`() {
        val a = row("a", "2026-09-01T10:00:00Z", issueId = "i1", member = member("n1"))
        val r = row("r", "2026-09-01T11:00:00Z", member = member("n1", "review"))
        val done = row("d", "2026-09-01T09:00:00Z", issueId = "i2", status = "ended", member = member("n2"))
        val tree = sessionTree(
            listOf(a, r, done),
            SessionTreeContext(
                workflows = workflows,
                workflowNodes = listOf(
                    workflowNode("w", "i1", id = "n1", state = "running"),
                    workflowNode("w", "i2", id = "n2", state = "landed"),
                    workflowNode("w", "i3", id = "n3", state = "blocked"),
                    workflowNode("w2", "i4", id = "other", state = "landed"),
                ),
            ),
        )
        val node = group(tree)
        assertEquals(2, node.liveRuns)
        assertEquals(1, node.nodesDone)
        assertEquals(3, node.nodesTotal)
        assertEquals(
            "2 running · 1 of 3 done",
            SessionTree.workflowGroupCaption(node.liveRuns, node.nodesDone, node.nodesTotal),
        )
    }

    @Test
    fun `still groups a stack beside a workflow, from the leftover top level`() {
        val a = row("a", issueId = "i1", member = member("n1"))
        val low = row("s-low", issueId = "i-low")
        val top = row("s-top", "2026-09-01T11:00:00Z", issueId = "i-top")
        val tree = sessionTree(
            listOf(a, low, top),
            SessionTreeContext(
                workflows = workflows,
                issues = listOf(
                    issue("i-low", "APP-1", "exp/APP-1", null),
                    issue("i-top", "APP-2", "exp/APP-2", "exp/APP-1"),
                ),
            ),
        )
        assertEquals(listOf("stack:i-low(s-low,s-top)", "workflow:w(a)"), shape(tree))
    }
}

// EXP-1068: the strings every client draws off the tree, byte-identical ×4.
class SessionTreeCaptionTest {
    @Test
    fun `says running and done`() {
        assertEquals("3 running · 5 of 8 done", SessionTree.workflowGroupCaption(3, 5, 8))
    }

    @Test
    fun `drops the running part with nothing live`() {
        assertEquals("5 of 8 done", SessionTree.workflowGroupCaption(0, 5, 8))
    }

    @Test
    fun `drops the done part before the nodes synced`() {
        assertEquals("1 running", SessionTree.workflowGroupCaption(1, 0, 0))
    }

    @Test
    fun `is empty with neither`() {
        assertEquals("", SessionTree.workflowGroupCaption(0, 0, 0))
    }

    @Test
    fun `reads the round off a review branch`() {
        assertEquals(3, SessionTree.reviewBranchRound("exp/wf-2f353e88-review-EXP-1068-r3"))
        assertEquals(12, SessionTree.reviewBranchRound("exp/wf-2f353e88-review-EXP-10-r12"))
    }

    @Test
    fun `is null for every other branch`() {
        assertEquals(null, SessionTree.reviewBranchRound("exp/EXP-1068"))
        assertEquals(null, SessionTree.reviewBranchRound("exp/wf-2f353e88-review-EXP-1068-r"))
        assertEquals(null, SessionTree.reviewBranchRound("exp/wf-2f353e88-review-EXP-1068-r0"))
        assertEquals(null, SessionTree.reviewBranchRound(null))
        assertEquals(null, SessionTree.reviewBranchRound(""))
    }

    @Test
    fun `reads the latest verdict for its round`() {
        assertEquals(
            SessionTree.ReviewRowVerdict.ChangesRequested,
            SessionTree.reviewRoundVerdict(2, 2, 2, "request_changes"),
        )
        assertEquals(
            SessionTree.ReviewRowVerdict.Approved,
            SessionTree.reviewRoundVerdict(2, 2, 2, "approve"),
        )
    }

    @Test
    fun `calls an older submitted round submitted`() {
        assertEquals(
            SessionTree.ReviewRowVerdict.Submitted,
            SessionTree.reviewRoundVerdict(1, 2, 2, "request_changes"),
        )
    }

    @Test
    fun `has no verdict for the pending round or without a node`() {
        val none = SessionTree.ReviewRowVerdict.None
        assertEquals(none, SessionTree.reviewRoundVerdict(3, 2, 2, "request_changes"))
        assertEquals(none, SessionTree.reviewRoundVerdict(null, 2, 2, "request_changes"))
        assertEquals(none, SessionTree.reviewRoundVerdict(1, null, null, null))
        assertEquals(none, SessionTree.reviewRoundVerdict(1, 0, null, null))
    }

    @Test
    fun `names the round and the verdict`() {
        assertEquals("Review r2 · approved", SessionTree.reviewRowCaption(2, SessionTree.ReviewRowVerdict.Approved, false))
        assertEquals(
            "Review r2 · changes requested",
            SessionTree.reviewRowCaption(2, SessionTree.ReviewRowVerdict.ChangesRequested, false),
        )
        assertEquals("Review r1 · submitted", SessionTree.reviewRowCaption(1, SessionTree.ReviewRowVerdict.Submitted, false))
    }

    @Test
    fun `says no verdict only once the run ended`() {
        assertEquals("Review r3", SessionTree.reviewRowCaption(3, SessionTree.ReviewRowVerdict.None, true))
        assertEquals("Review r3 · no verdict", SessionTree.reviewRowCaption(3, SessionTree.ReviewRowVerdict.None, false))
    }

    @Test
    fun `falls back to a bare Review without a round`() {
        assertEquals("Review", SessionTree.reviewRowCaption(null, SessionTree.ReviewRowVerdict.None, true))
        assertEquals("Review · no verdict", SessionTree.reviewRowCaption(null, SessionTree.ReviewRowVerdict.None, false))
    }

    // ── EXP-1108: the row marks, replayed from `session-tree-marks.json` by
    //    case name (web `session-tree.test.ts`, iOS, desktop the same).

    private val marks by lazy {
        kotlinx.serialization.json.Json.parseToJsonElement(contractFixtureJson("session-tree-marks.json")).jsonObject
    }

    private fun JsonElement?.str(): String? = (this as? JsonPrimitive)?.takeIf { it.isString }?.content

    private fun markDevice(json: JsonObject) = SessionMarkDevice(
        deviceId = json.getValue("deviceId").jsonPrimitive.content,
        userId = json["userId"].str(),
        defaultAgent = (json["launchDefaults"] as? JsonObject)?.get("defaultAgent").str(),
        defaultAccount = (json["launchDefaults"] as? JsonObject)?.get("defaultAccount").str(),
        profiles = (json["agentAccounts"] as? JsonObject).orEmpty().mapValues { (_, entry) ->
            (entry.jsonObject["profiles"] as? JsonArray).orEmpty().map { profile ->
                val p = profile.jsonObject
                SessionMarkProfile(
                    id = p.getValue("id").jsonPrimitive.content,
                    label = p["label"].str(),
                    active = p["active"]?.jsonPrimitive?.boolean ?: false,
                )
            }
        },
    )

    @Test
    fun `the workflow run account caption matches every fixture case`() {
        val cases = marks.getValue("accountCaptions").jsonArray
        assertTrue(cases.size >= 10)
        cases.forEach { element ->
            val case = element.jsonObject
            val name = case.getValue("name").jsonPrimitive.content
            val session = case.getValue("session").jsonObject
            val row = SessionMarkRow(
                agent = session["agent"].str(),
                agentAccount = session["agentAccount"].str(),
                deviceId = session["deviceId"].str(),
                userId = session["userId"].str(),
                workflowId = session["workflowId"].str(),
            )
            val devices = case.getValue("devices").jsonArray.map { markDevice(it.jsonObject) }
            assertEquals(name, case["caption"].str(), SessionTree.workflowRunAccountCaption(row, devices))
        }
    }

    @Test
    fun `the needs-you dot matches every fixture case`() {
        val cases = marks.getValue("needsYou").jsonArray
        assertTrue(cases.size >= 4)
        cases.forEach { element ->
            val case = element.jsonObject
            val question = case["pendingQuestion"]
            assertEquals(
                case.getValue("name").jsonPrimitive.content,
                case.getValue("needsYou").jsonPrimitive.boolean,
                SessionTree.sessionNeedsYou(
                    case.getValue("status").jsonPrimitive.content,
                    question != null && question !is JsonNull,
                ),
            )
        }
    }
}
