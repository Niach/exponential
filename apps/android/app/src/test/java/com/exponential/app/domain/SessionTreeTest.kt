package com.exponential.app.domain

import com.exponential.app.data.db.CodingSessionEntity
import kotlinx.serialization.json.JsonNull
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

// EXP-1050 — the NODE-based tree (`sessionTree`): resume collapse + parent
// nesting. The web table's cases by their `it(...)` names, so the ×4 lockstep
// reads straight across (web `session-tree.test.ts`, desktop
// `domain::session_tree`, iOS `SessionTreeNodeTests`).
class SessionTreeNodeTest {
    private fun row(
        id: String,
        at: String = "2026-09-01T10:00:00Z",
        resumedFromId: String? = null,
        parentSessionId: String? = null,
        issueId: String? = null,
        batchIssueIds: String? = null,
        status: String = "running",
    ) = CodingSessionEntity(
        id = id,
        teamId = "team-1",
        userId = "user-1",
        issueId = issueId,
        resumedFromId = resumedFromId,
        parentSessionId = parentSessionId,
        batchIssueIds = batchIssueIds,
        status = status,
        startedAt = at,
        createdAt = at,
        updatedAt = at,
    )

    /** The tree as `key(child,child)` strings — the web table's `ids` helper. */
    private fun shape(nodes: List<SessionTreeNode>): List<String> = nodes.map { node ->
        val key = sessionTreeNodeKey(node)
        if (node.children.isEmpty()) key else "$key(${shape(node.children).joinToString(",")})"
    }

    private fun epochMs(iso: String): Long = WireTimestamps.parseEpochMs(iso)!!

    @Test
    fun `walks children depth-first`() {
        val leaf = SessionTreeNode.Session(session = row("b"), chain = listOf(row("b")))
        val tree = listOf(
            SessionTreeNode.Session(session = row("p"), chain = listOf(row("p")), children = listOf(leaf)),
            SessionTreeNode.Session(session = row("a"), chain = listOf(row("a"))),
        )
        assertEquals(listOf("p", "b", "a"), flattenSessionTree(tree).map { it.session.id })
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

    // SLOP-3: issue runs, batch runs and chats are all plain rows — nothing
    // groups them but their parent.
    @Test
    fun `keeps issue, batch and chat runs as plain top-level rows`() {
        val issueRun = row("i", "2026-09-01T10:00:00Z", issueId = "i1")
        val batch = row("b", "2026-09-01T11:00:00Z", batchIssueIds = """["i2","i3"]""")
        val chat = row("c", "2026-09-01T12:00:00Z")
        assertEquals(listOf("c", "b", "i"), shape(sessionTree(listOf(issueRun, batch, chat))))
    }

    @Test
    fun `sorts top-level nodes by the activity of their whole subtree`() {
        val lone = row("lone", "2026-09-01T11:30:00Z")
        val parent = row("p", "2026-09-01T10:00:00Z")
        val child = row("c", "2026-09-01T12:00:00Z", parentSessionId = "p")
        val tree = sessionTree(listOf(lone, parent, child))
        assertEquals(listOf("p(c)", "lone"), shape(tree))
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
    private fun familyTree(): List<SessionTreeNode> {
        val parent = row("p", "2026-09-01T11:00:00Z")
        val child = row("c", "2026-09-01T11:30:00Z", parentSessionId = "p")
        val grand = row("g", "2026-09-01T11:40:00Z", parentSessionId = "c")
        val other = row("n2", "2026-09-01T10:00:00Z")
        return sessionTree(listOf(parent, child, grand, other))
    }

    @Test
    fun `flattens children with their depths`() {
        assertEquals(
            listOf("p" to 0, "c" to 1, "g" to 2, "n2" to 0),
            visibleSessionTreeRows(familyTree()).map { it.key to it.depth },
        )
    }

    @Test
    fun `hides everything under a collapsed node`() {
        val rows = visibleSessionTreeRows(familyTree(), setOf("c"))
        assertEquals(listOf("p", "c", "n2"), rows.map { it.key })
        assertEquals(true, rows.first { it.key == "c" }.hasChildren)
    }

    @Test
    fun `keys a session by its id`() {
        assertEquals("p", sessionTreeNodeKey(familyTree()[0]))
    }
}

// EXP-1108: the needs-you mark, replayed from `session-tree-marks.json` by case
// name (web `session-tree.test.ts`, iOS, desktop the same).
class SessionTreeMarksTest {
    private val marks by lazy {
        kotlinx.serialization.json.Json.parseToJsonElement(contractFixtureJson("session-tree-marks.json")).jsonObject
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
