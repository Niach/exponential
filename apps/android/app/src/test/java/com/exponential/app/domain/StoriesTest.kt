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
        put("description", nullable(story.description))
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
        put(
            "counts",
            story.counts?.let {
                buildJsonObject {
                    put("sections", it.sections)
                    put("pictures", it.pictures)
                    put("questions", it.questions)
                }
            } ?: JsonNull,
        )
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
        assertEquals(setOf("windowHours", "autoAdvanceMs"), constants.keys)
        assertEquals(constants.long("windowHours"), Stories.Constants.WINDOW_HOURS)
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
        assertEquals(progress.str("storyGap")!!.toFloat(), Stories.Progress.STORY_GAP)
        val viewer = fixture.getValue("viewer").jsonObject
        assertEquals(viewer.str("width")!!.toFloat(), Stories.Viewer.WIDTH)
        assertEquals(viewer.str("height")!!.toFloat(), Stories.Viewer.HEIGHT)
        assertEquals(viewer.str("radius")!!.toFloat(), Stories.Viewer.RADIUS)
        assertEquals(viewer.str("scrimHeight")!!.toFloat(), Stories.Viewer.SCRIM_HEIGHT)
        assertEquals(viewer.str("scrimFade")!!.toFloat(), Stories.Viewer.SCRIM_FADE)
        assertEquals(viewer.getValue("inset").jsonObject.str("x")!!.toFloat(), Stories.Viewer.INSET_X)
        assertEquals(viewer.getValue("inset").jsonObject.str("y")!!.toFloat(), Stories.Viewer.INSET_Y)
        assertEquals(viewer.str("minWidth")!!.toFloat(), Stories.Viewer.MIN_WIDTH)
        val row = fixture.getValue("row").jsonObject
        assertEquals(row.str("itemWidth")!!.toFloat(), Stories.Row.ITEM_WIDTH)
        assertEquals(row.str("gap")!!.toFloat(), Stories.Row.GAP)
        assertEquals(row.str("labelGap")!!.toFloat(), Stories.Row.LABEL_GAP)
        assertEquals(row.str("padBottom")!!.toFloat(), Stories.Row.PAD_BOTTOM)
        val badge = fixture.getValue("badge").jsonObject
        for (kind in Stories.Kind.entries) {
            assertEquals(badge.getValue("icons").jsonObject.str(kind.wire), Stories.Badge.icon(kind))
            assertEquals(badge.getValue("tints").jsonObject.str(kind.wire), Stories.Badge.tint(kind))
        }
        assertEquals(badge.str("fill")!!.toFloat(), Stories.Badge.FILL)
        assertEquals(badge.str("stroke")!!.toFloat(), Stories.Badge.STROKE)
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

    private fun sessionsOf(input: JsonObject) = input.getValue("sessions").jsonArray.map { it.jsonObject }.map {
        Stories.SessionInput(
            id = it.str("id")!!,
            rootId = it.str("rootId")!!,
            name = it.str("name")!!,
            agentId = it.str("agentId"),
            startedByAgentId = it.str("startedByAgentId"),
        )
    }

    private fun agentsOf(input: JsonObject) = input.getValue("agents").jsonArray.map { it.jsonObject }.map {
        Stories.AgentInput(it.str("id")!!, it.str("name")!!, it.getValue("assistant").jsonPrimitive.boolean)
    }

    private fun messagesOf(input: JsonObject) = input.getValue("messages").jsonArray.map { it.jsonObject }.map {
        Stories.MessageInput(
            id = it.str("id")!!,
            sessionId = it.str("sessionId")!!,
            authorSessionId = it.str("authorSessionId"),
            createdAt = it.long("createdAt")!!,
            answeredAt = it.long("answeredAt"),
            card = it["card"] as? JsonObject,
        )
    }

    private fun readsOf(input: JsonObject) = input.getValue("reads").jsonArray.map { it.jsonObject }.map {
        Stories.ReadInput(it.str("sessionId")!!, it.long("readAt"), it.long("storySeenAt"))
    }

    @Test
    fun `tab constants match the fixture`() {
        val tab = fixture.getValue("tab").jsonObject
        val copy = tab.getValue("copy").jsonObject
        assertEquals(
            mapOf(
                "label" to Stories.Tab.Copy.LABEL, "empty" to Stories.Tab.Copy.EMPTY,
                "approved" to Stories.Tab.Copy.APPROVED, "changed" to Stories.Tab.Copy.CHANGED,
                "answered" to Stories.Tab.Copy.ANSWERED, "superseded" to Stories.Tab.Copy.SUPERSEDED,
                "changes" to Stories.Tab.Copy.CHANGES, "stack" to Stories.Tab.Copy.STACK,
            ),
            copy.mapValues { it.value.jsonPrimitive.content },
        )
        val ids = tab.getValue("testIds").jsonObject
        assertEquals(
            mapOf(
                "tab" to Stories.Tab.TestIds.TAB, "list" to Stories.Tab.TestIds.LIST,
                "tile" to Stories.Tab.TestIds.TILE, "changes" to Stories.Tab.TestIds.CHANGES,
                "stack" to Stories.Tab.TestIds.STACK,
            ),
            ids.mapValues { it.value.jsonPrimitive.content },
        )
        val tile = tab.getValue("tile").jsonObject
        assertEquals(tile.getValue("thumbHeight").jsonObject.str("phone")!!.toFloat(), Stories.Tab.Tile.THUMB_HEIGHT)
        assertEquals(
            tile.getValue("thumbMaxWidth").jsonObject.str("phone")!!.toFloat(),
            Stories.Tab.Tile.THUMB_MAX_WIDTH,
        )
        assertEquals(tile.str("gap")!!.toFloat(), Stories.Tab.Tile.GAP)
        assertEquals(tile.str("textGap")!!.toFloat(), Stories.Tab.Tile.TEXT_GAP)
        assertEquals(tile.str("radius")!!.toFloat(), Stories.Tab.Tile.RADIUS)
        assertEquals(tile.str("listGap")!!.toFloat(), Stories.Tab.Tile.LIST_GAP)
        assertEquals(tile.str("leadGap")!!.toFloat(), Stories.Tab.Tile.LEAD_GAP)
        assertEquals(tab.getValue("specimen").jsonObject.long("case")!!.toInt(), Stories.Tab.SPECIMEN_CASE)
    }

    @Test
    fun `replays every tab case`() {
        val cases = fixture.getValue("tabCases").jsonArray.map { it.jsonObject }
        assertTrue(cases.isNotEmpty())
        for (case in cases) {
            val input = case.getValue("input").jsonObject
            val ring = Stories.storyTab(
                now = input.long("now")!!,
                sessions = sessionsOf(input),
                agents = agentsOf(input),
                messages = messagesOf(input),
                reads = readsOf(input),
                rootId = input.str("rootId")!!,
            )
            assertEquals(case.str("name"), case.getValue("expected"), ring?.let(::encode) ?: JsonNull)
        }
    }

    private fun storyOf(json: JsonObject) = Stories.Story(
        id = json.str("id")!!,
        sessionId = json.str("sessionId")!!,
        rootId = json.str("rootId")!!,
        sessionName = json.str("sessionName")!!,
        kind = Stories.Kind.entries.first { it.wire == json.str("kind") },
        waiting = json.getValue("waiting").jsonPrimitive.boolean,
        seen = json.getValue("seen").jsonPrimitive.boolean,
        createdAt = json.long("createdAt")!!,
        meta = json.str("meta")!!,
        title = json.str("title"),
        description = json.str("description"),
        pictures = json.getValue("pictures").jsonArray.map { it.jsonObject }.map {
            Stories.Picture(
                label = it.str("label")!!,
                attachmentId = it.str("attachmentId")!!,
                width = it.str("width")?.toDouble(),
                height = it.str("height")?.toDouble(),
                caption = it.str("caption"),
            )
        },
        surface = json.getValue("surface").jsonPrimitive.boolean,
        counts = (json["counts"] as? JsonObject)?.let {
            PlanPage.Counts(
                it.long("sections")!!.toInt(),
                it.long("pictures")!!.toInt(),
                it.long("questions")!!.toInt(),
            )
        },
        footer = Stories.Footer.entries.first { it.wire == json.str("footer") },
        autoAdvanceMs = json.long("autoAdvanceMs"),
    )

    private fun encode(frame: Stories.Frame) = buildJsonObject {
        put("story", frame.story)
        put("storyId", frame.storyId)
        put("kind", frame.kind.wire)
        put("pictures", frame.pictures)
        put("title", nullable(frame.title))
        put("description", nullable(frame.description))
        put("footer", frame.footer.wire)
        put("autoAdvanceMs", frame.autoAdvanceMs?.let { JsonPrimitive(it) } ?: JsonNull)
    }

    @Test
    fun `replays every frame case`() {
        val cases = fixture.getValue("frameCases").jsonArray.map { it.jsonObject }
        assertTrue(cases.isNotEmpty())
        for (case in cases) {
            val stories = case.getValue("input").jsonObject.getValue("stories").jsonArray.map { storyOf(it.jsonObject) }
            val frames = Stories.storyFrames(stories)
            assertEquals(case.str("name"), case.getValue("expected"), JsonArray(frames.map(::encode)))
        }
    }

    @Test
    fun `storyFrameIndex opens a tile (or a thumbnail) at its story's frame`() {
        for (case in fixture.getValue("frameCases").jsonArray.map { it.jsonObject }) {
            val frames = Stories.storyFrames(
                case.getValue("input").jsonObject.getValue("stories").jsonArray.map { storyOf(it.jsonObject) },
            )
            frames.forEachIndexed { at, frame -> assertEquals(at, Stories.storyFrameIndex(frames, frame.storyId)) }
            assertEquals(-1, Stories.storyFrameIndex(frames, "nope"))
        }
    }
}
