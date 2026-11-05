package com.exponential.app.domain

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.long
import kotlinx.serialization.json.put
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/** [ResultsTab] replays `fixtures/results-tab.json` `cases`, comparing the WHOLE
 *  expected array (the TS reference's own test). */
class ResultsTabTest {

    private val fixture: JsonObject by lazy {
        Json.parseToJsonElement(contractFixtureJson("results-tab.json")).jsonObject
    }

    private fun JsonObject.str(key: String): String? =
        this[key]?.takeUnless { it is JsonNull }?.jsonPrimitive?.content

    private fun JsonObject.long(key: String): Long? =
        this[key]?.takeUnless { it is JsonNull }?.jsonPrimitive?.long

    private fun nullable(value: String?): JsonElement = value?.let { JsonPrimitive(it) } ?: JsonNull

    private fun number(value: Double?): JsonElement = when {
        value == null -> JsonNull
        value % 1.0 == 0.0 -> JsonPrimitive(value.toLong())
        else -> JsonPrimitive(value)
    }

    private fun encode(result: ResultsTab.Result) = buildJsonObject {
        put("id", result.id)
        put("sessionId", result.sessionId)
        put("sessionName", result.sessionName)
        put("createdAt", result.createdAt)
        put("age", result.age)
        put("title", result.title)
        put("description", nullable(result.description))
        put(
            "pictures",
            JsonArray(
                result.pictures.map { picture ->
                    buildJsonObject {
                        put("label", picture.label)
                        put("attachmentId", picture.attachmentId)
                        put("width", number(picture.width))
                        put("height", number(picture.height))
                        put("caption", nullable(picture.caption))
                    }
                },
            ),
        )
        put("surface", result.surface)
    }

    @Test
    fun `constants match the fixture`() {
        assertEquals(
            mapOf("now" to ResultsTab.Copy.NOW, "untitled" to ResultsTab.Copy.UNTITLED),
            fixture.getValue("copy").jsonObject.mapValues { it.value.jsonPrimitive.content },
        )
        val tab = fixture.getValue("tab").jsonObject
        assertEquals(
            mapOf(
                "label" to ResultsTab.Tab.Copy.LABEL, "empty" to ResultsTab.Tab.Copy.EMPTY,
                "changes" to ResultsTab.Tab.Copy.CHANGES, "stack" to ResultsTab.Tab.Copy.STACK,
            ),
            tab.getValue("copy").jsonObject.mapValues { it.value.jsonPrimitive.content },
        )
        assertEquals(
            mapOf(
                "tab" to ResultsTab.Tab.TestIds.TAB, "list" to ResultsTab.Tab.TestIds.LIST,
                "section" to ResultsTab.Tab.TestIds.SECTION, "changes" to ResultsTab.Tab.TestIds.CHANGES,
                "stack" to ResultsTab.Tab.TestIds.STACK,
            ),
            tab.getValue("testIds").jsonObject.mapValues { it.value.jsonPrimitive.content },
        )
        val tile = tab.getValue("tile").jsonObject
        assertEquals(tile.getValue("thumbHeight").jsonObject.str("phone")!!.toFloat(), ResultsTab.Tab.Tile.THUMB_HEIGHT)
        assertEquals(tile.getValue("thumbMaxWidth").jsonObject.str("phone")!!.toFloat(), ResultsTab.Tab.Tile.THUMB_MAX_WIDTH)
        assertEquals(tile.str("gap")!!.toFloat(), ResultsTab.Tab.Tile.GAP)
        assertEquals(tile.str("textGap")!!.toFloat(), ResultsTab.Tab.Tile.TEXT_GAP)
        assertEquals(tile.str("radius")!!.toFloat(), ResultsTab.Tab.Tile.RADIUS)
        assertEquals(tile.str("listGap")!!.toFloat(), ResultsTab.Tab.Tile.LIST_GAP)
        assertEquals(tile.str("leadGap")!!.toFloat(), ResultsTab.Tab.Tile.LEAD_GAP)
        assertEquals(tab.getValue("specimen").jsonObject.long("case")!!.toInt(), ResultsTab.Tab.SPECIMEN_CASE)
    }

    @Test
    fun `replays every case`() {
        val cases = fixture.getValue("cases").jsonArray.map { it.jsonObject }
        assertTrue(cases.isNotEmpty())
        for (case in cases) {
            val input = case.getValue("input").jsonObject
            val results = ResultsTab.resultsTab(
                now = input.long("now")!!,
                rootId = input.str("rootId")!!,
                sessions = input.getValue("sessions").jsonArray.map { it.jsonObject }.map {
                    ResultsTab.SessionInput(it.str("id")!!, it.str("rootId")!!, it.str("name")!!)
                },
                messages = input.getValue("messages").jsonArray.map { it.jsonObject }.map {
                    ResultsTab.MessageInput(
                        id = it.str("id")!!,
                        sessionId = it.str("sessionId")!!,
                        authorSessionId = it.str("authorSessionId"),
                        createdAt = it.long("createdAt")!!,
                        card = it["card"] as? JsonObject,
                    )
                },
            )
            assertEquals(case.str("name"), case.getValue("expected"), JsonArray(results.map(::encode)))
        }
    }
}
