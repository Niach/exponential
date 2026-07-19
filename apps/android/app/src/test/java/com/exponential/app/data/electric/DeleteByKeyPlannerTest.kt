package com.exponential.app.data.electric

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * Unit tests for the key-based delete fallback (EXP-26). Electric `delete` /
 * move-out messages carry PK-only payloads that fail full-entity decode, so
 * the row must be deletable from the Electric key alone — including for
 * composite-PK tables like issue_labels.
 */
class DeleteByKeyPlannerTest {

    @Test
    fun parsesSinglePkKey() {
        assertEquals(
            listOf("f7a4c2e1-1111-2222-3333-444455556666"),
            parseKeyComponents("\"public\".\"issues\"/\"f7a4c2e1-1111-2222-3333-444455556666\""),
        )
    }

    @Test
    fun parsesCompositePkKeyInOrder() {
        assertEquals(
            listOf("issue-1", "label-2"),
            parseKeyComponents("\"public\".\"issue_labels\"/\"issue-1\"/\"label-2\""),
        )
    }

    @Test
    fun keyWithoutValueSegmentsHasNoComponents() {
        assertTrue(parseKeyComponents("\"public\".\"issues\"").isEmpty())
    }

    @Test
    fun plansSinglePkDelete() {
        val plan = planDeleteByKey(
            pkColumns = listOf("id"),
            key = "\"public\".\"boards\"/\"p-1\"",
        )
        assertEquals("\"id\" = ?", plan!!.whereClause)
        assertEquals(listOf("p-1"), plan.args)
    }

    @Test
    fun plansCompositePkDelete() {
        val plan = planDeleteByKey(
            pkColumns = listOf("issue_id", "label_id"),
            key = "\"public\".\"issue_labels\"/\"i-1\"/\"l-1\"",
        )
        assertEquals("\"issue_id\" = ? AND \"label_id\" = ?", plan!!.whereClause)
        assertEquals(listOf("i-1", "l-1"), plan.args)
    }

    @Test
    fun refusesArityMismatch() {
        // Two PK columns but one key value — nothing safe to target.
        assertNull(
            planDeleteByKey(
                pkColumns = listOf("issue_id", "label_id"),
                key = "\"public\".\"issue_labels\"/\"i-1\"",
            ),
        )
        // One PK column but two key values.
        assertNull(
            planDeleteByKey(
                pkColumns = listOf("id"),
                key = "\"public\".\"issues\"/\"a\"/\"b\"",
            ),
        )
    }

    @Test
    fun refusesEmptyPkColumns() {
        assertNull(planDeleteByKey(pkColumns = emptyList(), key = "\"public\".\"t\"/\"x\""))
    }
}
