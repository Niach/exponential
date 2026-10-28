package com.exponential.app.domain

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.long
import kotlinx.serialization.json.put
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/** [Stories] replays `fixtures/stories.json` `cases`, comparing the WHOLE
 *  expected array (the TS reference's own test). */
class StoriesTest {

    private val fixture: JsonObject by lazy {
        Json.parseToJsonElement(contractFixtureJson("stories.json")).jsonObject
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

    private fun encode(story: Stories.Story) = buildJsonObject {
        put("id", story.id)
        put("sessionId", story.sessionId)
        put("rootId", story.rootId)
        put("sessionName", story.sessionName)
        put("kind", story.kind.wire)
        put("waiting", story.waiting)
        put("seen", story.seen)
        put("createdAt", story.createdAt)
        put("meta", story.meta)
        put("title", nullable(story.title))
        put(
            "pictures",
            JsonArray(
                story.pictures.map { picture ->
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
        put("surface", story.surface)
        put("text", nullable(story.text))
        put("footer", story.footer.wire)
        put("autoAdvanceMs", story.autoAdvanceMs?.let { JsonPrimitive(it) } ?: JsonNull)
    }

    private fun encode(ring: Stories.StoryRing) = buildJsonObject {
        put("key", ring.key)
        put("agentId", nullable(ring.agentId))
        put("rootId", nullable(ring.rootId))
        put("title", ring.title)
        put("assistant", ring.assistant)
        put("waiting", ring.waiting)
        put("unseen", ring.unseen)
        put("stories", JsonArray(ring.stories.map(::encode)))
    }

    @Test
    fun `constants match the fixture`() {
        val constants = fixture.getValue("constants").jsonObject
        assertEquals(constants.long("seenHours"), Stories.Constants.SEEN_HOURS)
        assertEquals(constants.long("unseenHours"), Stories.Constants.UNSEEN_HOURS)
        assertEquals(constants.long("autoAdvanceMs"), Stories.Constants.AUTO_ADVANCE_MS)
        val copy = fixture.getValue("copy").jsonObject
        assertEquals(
            mapOf(
                "plan" to Stories.Copy.PLAN, "question" to Stories.Copy.QUESTION, "result" to Stories.Copy.RESULT,
                "now" to Stories.Copy.NOW, "approve" to Stories.Copy.APPROVE, "change" to Stories.Copy.CHANGE,
                "reply" to Stories.Copy.REPLY, "close" to Stories.Copy.CLOSE, "next" to Stories.Copy.NEXT,
                "previous" to Stories.Copy.PREVIOUS,
            ),
            copy.mapValues { it.value.jsonPrimitive.content },
        )
        val ring = fixture.getValue("ring").jsonObject
        assertEquals(ring.str("width")!!.toFloat(), Stories.Ring.WIDTH)
        assertEquals(ring.str("gap")!!.toFloat(), Stories.Ring.GAP)
        val progress = fixture.getValue("progress").jsonObject
        assertEquals(progress.str("height")!!.toFloat(), Stories.Progress.HEIGHT)
        assertEquals(progress.str("gap")!!.toFloat(), Stories.Progress.GAP)
        val viewer = fixture.getValue("viewer").jsonObject
        assertEquals(viewer.str("width")!!.toFloat(), Stories.Viewer.WIDTH)
        assertEquals(viewer.str("height")!!.toFloat(), Stories.Viewer.HEIGHT)
        assertEquals(viewer.str("radius")!!.toFloat(), Stories.Viewer.RADIUS)
        val ids = fixture.getValue("testIds").jsonObject
        assertEquals(
            mapOf(
                "ring" to Stories.TestIds.RING, "viewer" to Stories.TestIds.VIEWER,
                "progress" to Stories.TestIds.PROGRESS, "header" to Stories.TestIds.HEADER,
                "next" to Stories.TestIds.NEXT, "previous" to Stories.TestIds.PREVIOUS,
            ),
            ids.mapValues { it.value.jsonPrimitive.content },
        )
    }

    @Test
    fun `replays every case`() {
        val cases = fixture.getValue("cases").jsonArray.map { it.jsonObject }
        assertTrue(cases.isNotEmpty())
        for (case in cases) {
            val input = case.getValue("input").jsonObject
            val rings = Stories.rings(
                now = input.long("now")!!,
                sessions = input.getValue("sessions").jsonArray.map { it.jsonObject }.map {
                    Stories.SessionInput(
                        id = it.str("id")!!,
                        rootId = it.str("rootId")!!,
                        name = it.str("name")!!,
                        agentId = it.str("agentId"),
                        startedByAgentId = it.str("startedByAgentId"),
                    )
                },
                agents = input.getValue("agents").jsonArray.map { it.jsonObject }.map {
                    Stories.AgentInput(it.str("id")!!, it.str("name")!!, it.getValue("assistant").jsonPrimitive.boolean)
                },
                messages = input.getValue("messages").jsonArray.map { it.jsonObject }.map {
                    Stories.MessageInput(
                        id = it.str("id")!!,
                        sessionId = it.str("sessionId")!!,
                        authorSessionId = it.str("authorSessionId"),
                        createdAt = it.long("createdAt")!!,
                        answeredAt = it.long("answeredAt"),
                        card = it["card"] as? JsonObject,
                    )
                },
                reads = input.getValue("reads").jsonArray.map { it.jsonObject }.map {
                    Stories.ReadInput(it.str("sessionId")!!, it.long("readAt"), it.long("storySeenAt"))
                },
            )
            assertEquals(case.str("name"), case.getValue("expected"), JsonArray(rings.map(::encode)))
        }
    }
}
