package com.exponential.app.domain

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.double
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.put
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/** [Stories.storyRow] replays `rowCases` (`bubbles` + the `rest` keys) and
 *  [Stories.storyViewerSize] replays `sizeCases`, whole values (#EXP-1461,
 *  #EXP-1463). */
class StoriesRowTest {

    private val fixture: JsonObject by lazy {
        Json.parseToJsonElement(contractFixtureJson("stories.json")).jsonObject
    }

    private fun ring(json: JsonObject): Stories.StoryRing {
        val key = json.getValue("key").jsonPrimitive.content
        val agentId = json["agentId"]?.takeUnless { it is JsonNull }?.jsonPrimitive?.content
        return Stories.StoryRing(
            key = key,
            agentId = agentId,
            rootId = if (agentId == null) key.removePrefix("root:") else null,
            title = key,
            assistant = false,
            waiting = 0,
            unseen = true,
            stories = emptyList(),
        )
    }

    @Test
    fun `replays every row case`() {
        val cases = fixture.getValue("rowCases").jsonArray.map { it.jsonObject }
        assertTrue(cases.isNotEmpty())
        for (case in cases) {
            val input = case.getValue("input").jsonObject
            val row = Stories.storyRow(
                input.getValue("bubbleAgentIds").jsonArray.map { it.jsonPrimitive.content },
                input.getValue("rings").jsonArray.map { ring(it.jsonObject) },
            )
            val actual = buildJsonObject {
                put("bubbles", JsonArray(row.bubbles.map(::JsonPrimitive)))
                put("rest", JsonArray(row.rest.map { JsonPrimitive(it.key) }))
            }
            assertEquals(case.getValue("name").jsonPrimitive.content, case.getValue("expected"), actual)
            row.bubbles.forEach { assertEquals("agent:$it", row.byAgent.getValue(it).key) }
        }
    }

    @Test
    fun `the first ring of an agent wins`() {
        val first = ring(buildJsonObject { put("key", "agent:ag1"); put("agentId", "ag1") })
        val second = ring(buildJsonObject { put("key", "agent:ag1-b"); put("agentId", "ag1") })
        val row = Stories.storyRow(listOf("ag1"), listOf(first, second))
        assertEquals(mapOf("ag1" to first), row.byAgent)
        assertEquals(listOf(second), row.rest)
    }

    @Test
    fun `replays every size case`() {
        val cases = fixture.getValue("sizeCases").jsonArray.map { it.jsonObject }
        assertTrue(cases.isNotEmpty())
        for (case in cases) {
            val input = case.getValue("input").jsonObject
            val picture = input["picture"]?.takeUnless { it is JsonNull }?.jsonObject
            val viewport = input.getValue("viewport").jsonObject
            fun JsonObject.num(key: String) = this[key]?.takeUnless { it is JsonNull }?.jsonPrimitive?.double
            val size = Stories.storyViewerSize(
                picture?.num("width"),
                picture?.num("height"),
                viewport.num("width")!!,
                viewport.num("height")!!,
            )
            val actual = buildJsonObject {
                put("width", size.width)
                put("height", size.height)
            }
            assertEquals(case.getValue("name").jsonPrimitive.content, case.getValue("expected"), actual)
        }
    }
}
