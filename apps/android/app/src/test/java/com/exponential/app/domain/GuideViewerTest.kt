package com.exponential.app.domain

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.put
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/** EXP-1283: `session-results.json` `viewer.cases`, compared as a WHOLE. */
class GuideViewerTest {

    private fun nullable(value: String?): JsonElement = value?.let(::JsonPrimitive) ?: JsonNull

    private fun encode(state: GuideViewer.State?): JsonElement = state?.let {
        buildJsonObject {
            put("index", it.index)
            put("count", it.count)
            put("topic", it.topic)
            put("label", it.label)
            put("text", nullable(it.text))
            put("prevId", nullable(it.prevId))
            put("nextId", nullable(it.nextId))
        }
    } ?: JsonNull

    @Test
    fun `every viewer case matches the fixture`() {
        val fixture = Json.parseToJsonElement(contractFixtureJson("session-results.json")).jsonObject
        val cases = fixture.getValue("viewer").jsonObject.getValue("cases").jsonArray.map { it.jsonObject }
        assertTrue(cases.isNotEmpty())
        for (case in cases) {
            val actual = GuideViewer.state(case.getValue("guide").toString(), case.getValue("attachmentId").jsonPrimitive.content)
            assertEquals(case.getValue("name").jsonPrimitive.content, case.getValue("expected"), encode(actual))
        }
    }
}
