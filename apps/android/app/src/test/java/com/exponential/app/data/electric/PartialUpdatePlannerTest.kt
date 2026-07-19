package com.exponential.app.data.electric

import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * Unit tests for the pure tolerant partial-apply planner. These pin the two
 * bricking cases the sync overhaul fixes (an unknown server column, and a
 * composite-PK table) plus the JSON→SQLite arg binding.
 */
class PartialUpdatePlannerTest {

    @Test
    fun filtersUnknownColumnsAndKeepsKnownOrder() {
        val wire: Map<String, JsonElement> = linkedMapOf(
            "title" to JsonPrimitive("hi"),
            "onboarding_completed_at" to JsonPrimitive("2026-01-01"),
            "status" to JsonPrimitive("todo"),
        )
        val plan = planPartialUpdate(
            pkColumns = listOf("id"),
            knownColumns = setOf("id", "title", "status"),
            wireColumns = wire,
        )
        assertEquals("\"title\" = ?, \"status\" = ?", plan!!.setClause)
        assertEquals(listOf<Any?>("hi", "todo"), plan.args)
        assertEquals(setOf("onboarding_completed_at"), plan.droppedColumns)
    }

    @Test
    fun skipsCompositePrimaryKeyTables() {
        val plan = planPartialUpdate(
            pkColumns = listOf("issue_id", "label_id"),
            knownColumns = setOf("issue_id", "label_id", "team_id"),
            wireColumns = linkedMapOf("team_id" to JsonPrimitive("w1")),
        )
        assertNull(plan)
    }

    @Test
    fun skipsTablesWhosePrimaryKeyIsNotId() {
        val plan = planPartialUpdate(
            pkColumns = listOf("shape"),
            knownColumns = setOf("shape", "handle"),
            wireColumns = linkedMapOf("handle" to JsonPrimitive("h")),
        )
        assertNull(plan)
    }

    @Test
    fun allUnknownColumnsProduceAnEmptyNoOpPlan() {
        val plan = planPartialUpdate(
            pkColumns = listOf("id"),
            knownColumns = setOf("id"),
            wireColumns = linkedMapOf(
                "creem_customer_id" to JsonPrimitive("c"),
                "had_trial" to JsonPrimitive(true),
            ),
        )
        assertTrue(plan!!.setClause.isEmpty())
        assertTrue(plan.args.isEmpty())
        assertEquals(setOf("creem_customer_id", "had_trial"), plan.droppedColumns)
    }

    @Test
    fun bindsJsonNullToSqlNull() {
        val plan = planPartialUpdate(
            pkColumns = listOf("id"),
            knownColumns = setOf("id", "assignee_id"),
            wireColumns = linkedMapOf("assignee_id" to JsonNull),
        )
        assertEquals("\"assignee_id\" = ?", plan!!.setClause)
        assertEquals(listOf<Any?>(null), plan.args)
        assertTrue(plan.droppedColumns.isEmpty())
    }

    @Test
    fun bindsUnquotedBooleansAsSqliteIntegers() {
        val plan = planPartialUpdate(
            pkColumns = listOf("id"),
            knownColumns = setOf("id", "unsubscribed"),
            wireColumns = linkedMapOf("unsubscribed" to JsonPrimitive(true)),
        )
        // 1L, not "true": Room Boolean columns are INTEGER-affinity.
        assertEquals(listOf<Any?>(1L), plan!!.args)

        val off = planPartialUpdate(
            pkColumns = listOf("id"),
            knownColumns = setOf("id", "unsubscribed"),
            wireColumns = linkedMapOf("unsubscribed" to JsonPrimitive(false)),
        )
        assertEquals(listOf<Any?>(0L), off!!.args)
    }

    @Test
    fun keepsAQuotedStringThatLooksLikeABoolean() {
        val plan = planPartialUpdate(
            pkColumns = listOf("id"),
            knownColumns = setOf("id", "title"),
            wireColumns = linkedMapOf("title" to JsonPrimitive("true")),
        )
        assertEquals(listOf<Any?>("true"), plan!!.args)
    }

    @Test
    fun ignoresTheIdColumnInTheSetList() {
        val plan = planPartialUpdate(
            pkColumns = listOf("id"),
            knownColumns = setOf("id", "title"),
            wireColumns = linkedMapOf(
                "id" to JsonPrimitive("row-1"),
                "title" to JsonPrimitive("t"),
            ),
        )
        assertEquals("\"title\" = ?", plan!!.setClause)
        assertEquals(listOf<Any?>("t"), plan.args)
        assertTrue(plan.droppedColumns.isEmpty())
    }
}
