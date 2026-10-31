package com.exponential.app.domain

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.long
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * EXP-1501: [workingCaption] replays `fixtures/working-caption.json` and
 * compares the WHOLE caption, like the TS reference's own test, desktop
 * `ui::session_rows::working_caption` and iOS `AgentWorkflow.workingCaption`.
 */
class WorkingCaptionTest {

    private val fixture: JsonObject by lazy {
        Json.parseToJsonElement(contractFixtureJson("working-caption.json")).jsonObject
    }

    private fun JsonObject.str(key: String): String? =
        this[key]?.takeUnless { it is JsonNull }?.jsonPrimitive?.content

    private fun JsonObject.lng(key: String): Long? =
        this[key]?.takeUnless { it is JsonNull }?.jsonPrimitive?.long

    private fun JsonObject.obj(key: String): JsonObject? =
        this[key]?.takeUnless { it is JsonNull }?.jsonObject

    @Test
    fun `the copy matches the fixture`() {
        val copy = fixture.obj("copy")!!
        assertEquals(copy.str("fallback"), WORKING_FALLBACK_CAPTION)
        assertEquals(copy.str("writing"), WORKING_WRITING_CAPTION)
        assertEquals(copy.str("writingBlank"), WORKING_WRITING_BLANK_CAPTION)
    }

    @Test
    fun `every case reads the whole caption`() {
        val cases = fixture.getValue("cases").jsonArray.map { it.jsonObject }
        assertTrue(cases.size >= 8)
        for (case in cases) {
            val input = case.obj("input")!!
            val writing = input.obj("writing")?.let {
                WorkingWriting(tool = it.str("tool")!!, startedAt = it.lng("startedAt")!!)
            }
            val actual = workingCaption(
                startedAt = input.lng("startedAt"),
                tokens = input.lng("tokens"),
                nowMs = input.lng("now")!!,
                writing = writing,
            )
            assertEquals(case.str("name"), case.str("expected"), actual)
        }
    }

    @Test
    fun `the tool label`() {
        assertEquals("exponential_show", writingToolLabel("mcp__exponential__exponential_show"))
        assertEquals("Write", writingToolLabel(" Write\n"))
        assertEquals(null, writingToolLabel("  "))
        // A lone `mcp__` prefix with no second separator is just a name.
        assertEquals("mcp__odd", writingToolLabel("mcp__odd"))
    }
}
