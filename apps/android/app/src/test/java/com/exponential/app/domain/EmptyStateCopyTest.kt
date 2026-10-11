package com.exponential.app.domain

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** EXP-1274: [EmptyStateCopy] == `empty-state.json`, both directions. */
class EmptyStateCopyTest {

    private val fixture: JsonObject =
        Json.parseToJsonElement(contractFixtureJson("empty-state.json")).jsonObject
    private val states: JsonObject = fixture.getValue("states").jsonObject

    private fun JsonObject.optString(key: String): String? = this[key]?.jsonPrimitive?.content

    @Test
    fun everyConstantMatchesTheFixture() {
        for (state in EmptyStateCopy.all) {
            val row = states[state.id]?.jsonObject ?: error("${state.id} missing from the fixture")
            val size = if (row.getValue("size").jsonPrimitive.content == "page") {
                EmptyStateCopy.Size.Page
            } else {
                EmptyStateCopy.Size.List
            }
            assertEquals(state.id, size, state.size)
            assertEquals(state.id, row.getValue("icon").jsonPrimitive.content, state.iconConcept)
            assertNotNull(state.id, EmptyStateCopy.conceptIcon(state.iconConcept))
            assertEquals(state.id, row.getValue("title").jsonPrimitive.content, state.title)
            assertEquals(state.id, row.optString("description"), state.description)
            assertEquals(state.id, row.optString("action"), state.action)
        }
    }

    @Test
    fun theFixtureHasNoUndeclaredState() {
        val declared = EmptyStateCopy.all.map { it.id }
        assertEquals(declared.size, declared.toSet().size)
        assertEquals(states.keys, declared.toSet())
    }

    @Test
    fun copyRulesHold() {
        for (state in EmptyStateCopy.all) {
            assertFalse(state.id, state.title.endsWith("."))
            if (state.size == EmptyStateCopy.Size.List) {
                assertNull(state.id, state.description)
                assertNull(state.id, state.action)
            }
        }
    }

    @Test
    fun sizesMatchTheFixture() {
        val sizes = fixture.getValue("sizes").jsonObject
        val page = sizes.getValue("page").jsonObject
        val list = sizes.getValue("list").jsonObject
        assertEquals(48, page.getValue("disc").jsonPrimitive.int)
        assertEquals(24, page.getValue("glyph").jsonPrimitive.int)
        assertEquals(48, page.getValue("paddingY").jsonPrimitive.int)
        assertEquals(448, page.getValue("maxWidth").jsonPrimitive.int)
        assertEquals(20, list.getValue("glyph").jsonPrimitive.int)
        assertEquals(24, list.getValue("paddingY").jsonPrimitive.int)
    }
}
