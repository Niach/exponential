package com.exponential.app.domain

import org.junit.Assert.assertEquals
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

    @Test
    fun breaksACycleWhereItFirstAppears() {
        assertEquals(
            listOf("self@0", "a@0+", "b@1"),
            shape(nest(listOf(Row("a", "b"), Row("b", "a"), Row("self", "self")))),
        )
    }
}
