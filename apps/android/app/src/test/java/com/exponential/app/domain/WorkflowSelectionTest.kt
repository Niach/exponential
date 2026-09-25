package com.exponential.app.domain

import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/** EXP-1084: the workflow page's picker, locked by the fixture's `selection` cases. */
class WorkflowSelectionTest {

    private val fixture = Json.parseToJsonElement(contractFixtureJson("workflow-view.json")).jsonObject

    private fun JsonElement?.stringOrNull(): String? = (this as? JsonPrimitive)?.takeIf { it.isString }?.content

    private fun selectionOf(raw: JsonObject) = WorkflowSelection(
        ids = raw.getValue("ids").jsonArray.map { it.jsonPrimitive.content },
        anchor = raw["anchor"].stringOrNull(),
        cursor = raw["cursor"].stringOrNull(),
    )

    @Test
    fun `every selection case lands where the fixture says`() {
        val cases = fixture.getValue("selection").jsonArray
        assertTrue(cases.size >= 17)
        cases.forEach { element ->
            val case = element.jsonObject
            val name = case.getValue("name").jsonPrimitive.content
            val order = case.getValue("order").jsonArray.map { it.jsonPrimitive.content }
            val current = selectionOf(case.getValue("current").jsonObject)
            val op = case.getValue("op").jsonObject
            val id = op["id"].stringOrNull()
            val actual = when (val kind = op.getValue("kind").jsonPrimitive.content) {
                "click" -> current.click(id, order)
                "toggle" -> current.toggle(id!!, order)
                "extend" -> current.extend(id!!, order)
                "step" -> current.step(op.getValue("delta").jsonPrimitive.int, order)
                "prune" -> current.prune(order)
                else -> error("unknown op $kind")
            }
            assertEquals(name, selectionOf(case.getValue("expected").jsonObject), actual)
        }
    }

    @Test
    fun `the order is waves then lanes`() {
        fun chip(id: String) = NodeChip(id, id, WorkflowNodeDisplayState.QUEUED, "Queued", false, 0, false, false)
        val strip = listOf(
            StripWave(0, listOf(chip("c"))),
            StripWave(1, listOf(chip("a"), chip("b"))),
            StripWave(2, listOf(chip("d"))),
        )
        assertEquals(listOf("c", "a", "b", "d"), WorkflowSelection.order(strip))
    }

    @Test
    fun `the position counts All as zero and follows the cursor`() {
        val order = listOf("c", "a", "b", "d")
        assertEquals(0, WorkflowSelection.ALL.position(order))
        assertEquals(3, WorkflowSelection.ALL.click("b", order).position(order))
        assertEquals(1, WorkflowSelection(listOf("c", "a", "b"), "b", "c").position(order))
    }
}
