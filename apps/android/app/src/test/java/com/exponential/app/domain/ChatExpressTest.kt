package com.exponential.app.domain

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.put
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/**
 * #EXP-2073: `chat-express.json` replayed over the eui FFI (the host build of
 * exponential-ui-ffi, loaded by JNA from `jna.library.path`): segmentCases
 * (the whole expected array, `error` as the category), streamCases step by
 * step + any chunking, actionCases, inputCases, failureNoteCases,
 * plainProseCases and previewCases ×4 (web `chat-express.test.ts`).
 */
class ChatExpressTest {
    private val fixture: JsonObject = Json.parseToJsonElement(contractFixtureJson("chat-express.json")).jsonObject
    private val sdk = ChatExpressFfi

    private fun cases(key: String): List<JsonObject> = fixture.getValue(key).jsonArray.map { it.jsonObject }
    private fun JsonObject.str(key: String): String? = (this[key] as? JsonPrimitive)?.takeIf { it.isString }?.content

    /** A row as the fixture writes it (no messages, no generation, no SDK words). */
    private fun locked(segments: List<ChatExpress.RowSegment>?): JsonElement {
        if (segments == null) return JsonNull
        return JsonArray(
            segments.map { s ->
                when (s) {
                    is ChatExpress.RowSegment.Prose -> buildJsonObject {
                        put("kind", "prose")
                        put("text", s.text)
                    }
                    is ChatExpress.RowSegment.Block -> buildJsonObject {
                        put("kind", "block")
                        put("key", s.key)
                        put("raw", s.raw)
                        put("closed", s.closed)
                        put("status", s.status.wire)
                        s.error?.let { put("error", it) }
                        put("surfaces", JsonArray(s.surfaces.map(::JsonPrimitive)))
                    }
                }
            },
        )
    }

    @Test
    fun `the limits, copy and gate are the fixture's`() {
        val limits = fixture.getValue("limits").jsonObject
        assertEquals(ChatExpress.Limits.BLOCKS_PER_ROW_MAX, limits.getValue("blocksPerRowMax").jsonPrimitive.int)
        assertEquals(ChatExpress.Limits.BLOCK_MAX_BYTES, limits.getValue("blockMaxBytes").jsonPrimitive.int)
        assertEquals(ChatExpress.Limits.BLOCK_MAX_SURFACES, limits.getValue("blockMaxSurfaces").jsonPrimitive.int)
        assertEquals(ChatExpress.Limits.BLOCK_MAX_COMPONENTS, limits.getValue("blockMaxComponents").jsonPrimitive.int)
        assertEquals(ChatExpress.Limits.ACTION_CONTEXT_MAX_BYTES, limits.getValue("actionContextMaxBytes").jsonPrimitive.int)
        assertEquals(ChatExpress.Limits.ACTION_NAME_MAX, limits.getValue("actionNameMax").jsonPrimitive.int)
        assertEquals(ChatExpress.Limits.NOTE_LINE_MAX, limits.getValue("noteLineMax").jsonPrimitive.int)
        val copy = fixture.getValue("copy").jsonObject
        assertEquals(ChatExpress.Copy.BLOCK_FAILED, copy.str("blockFailed"))
        assertEquals(ChatExpress.Copy.BLOCK_ONLY, copy.str("blockOnly"))
        assertEquals(ChatExpress.Copy.ACTION_MARKER, copy.str("actionMarker"))
        assertEquals(ChatExpress.Copy.YOU, copy.str("you"))
        assertEquals(ChatExpress.Copy.ACTION_TOO_LARGE, copy.str("actionTooLarge"))
        assertEquals(ChatExpress.GATE, fixture.str("gate"))
        assertEquals(ChatExpress.REPLY_EVENT, fixture.str("replyEvent"))
        assertEquals(ChatExpress.BLOCK_GAP, fixture.getValue("layout").jsonObject.getValue("blockGap").jsonPrimitive.int)
    }

    @Test
    fun segmentCases() {
        for (c in cases("segmentCases")) {
            val text = c.str("text")!!
            val row = ChatExpress.expressRow(text, c.str("rowKey")!!, c.getValue("final").jsonPrimitive.boolean, sdk)
            assertEquals(c.str("name"), c.getValue("expected"), locked(row))
            assertEquals(c.str("name"), c.str("prose"), ChatExpress.expressProse(row, text))
        }
    }

    @Test
    fun `streamCases one row, fragment by fragment`() {
        val steps = cases("streamCases")
        val row = ChatExpress.ExpressRow(steps[0].str("rowKey")!!, sdk)
        for (step in steps) {
            val out = row.update(step.str("text")!!, step.getValue("final").jsonPrimitive.boolean)
            assertEquals(step.str("text"), step.getValue("expected"), locked(out))
            assertEquals(step.str("prose"), ChatExpress.expressProse(out, step.str("text")!!))
        }
    }

    @Test
    fun `any chunking reaches the segmentCases states`() {
        for (c in cases("segmentCases")) {
            val text = c.str("text")!!
            for (size in listOf(1, 3, 17)) {
                val row = ChatExpress.ExpressRow(c.str("rowKey")!!, sdk)
                var i = size
                while (i < text.length + size) {
                    row.update(text.substring(0, minOf(i, text.length)), false)
                    i += size
                }
                val out = row.update(text, c.getValue("final").jsonPrimitive.boolean)
                assertEquals("${c.str("name")} by $size", c.getValue("expected"), locked(out))
            }
        }
    }

    private fun encode(result: ChatExpress.ActionResult?): JsonElement = when (result) {
        null -> JsonNull
        ChatExpress.ActionResult.TooLarge -> JsonPrimitive("tooLarge")
        is ChatExpress.ActionResult.Post -> buildJsonObject {
            put("body", result.body)
            put("card", result.card.toJson())
        }
    }

    @Test
    fun actionCases() {
        for (c in cases("actionCases")) {
            val event = c.getValue("event").jsonObject
            val origin = c.getValue("origin").jsonObject
            val actual = ChatExpress.surfaceAction(
                ChatExpress.ActionEvent(event["name"], event["context"], event["userMessage"]),
                ChatExpress.ActionOrigin(origin.str("surfaceId")!!, origin.str("block"), origin.str("messageId")),
            )
            assertEquals(c.str("name"), c.getValue("expected"), encode(actual))
        }
    }

    @Test
    fun `the show origin is the show surface`() {
        assertEquals(ChatExpress.ActionOrigin("show:m9", null, "m9"), ChatExpress.showActionOrigin("m9"))
    }

    @Test
    fun inputCases() {
        for (c in cases("inputCases")) {
            val card = ChatExpress.ActionCard.of(c.getValue("card").jsonObject)!!
            val body = c.str("body")!!
            assertEquals(c.str("name"), c.str("expected"), ChatExpress.actionInput(card, body))
            assertEquals(c.str("name"), ChatExpress.ParsedInput(card, body), ChatExpress.parseActionInput(c.str("expected")!!))
        }
        assertNull(ChatExpress.parseActionInput("[action] not json"))
        assertNull(ChatExpress.parseActionInput("Ship it"))
    }

    @Test
    fun failureNoteCases() {
        for (c in cases("failureNoteCases")) {
            val texts = c.getValue("texts").jsonArray.map { it.jsonPrimitive.content }
            assertEquals(c.str("name"), c.str("expected"), ChatExpress.failureNote(texts, sdk))
        }
    }

    @Test
    fun plainProseCases() {
        for (c in cases("plainProseCases")) {
            assertEquals(c.str("name"), c.str("expected"), ChatExpress.plainProse(c.str("text")!!))
        }
    }

    @Test
    fun previewCases() {
        for (c in cases("previewCases")) {
            val message = SessionTree.Message(
                id = "m1",
                sessionId = "r",
                authorKind = c.str("authorKind")!!,
                authorId = null,
                body = c.str("body")!!,
                card = c["card"] as? JsonObject,
                answeredAt = null,
                consumedAt = "2026-11-23T08:00:00.000Z",
                createdAt = "2026-11-23T09:00:00.000Z",
            )
            val preview = Chats.chatPreview(rootId = "r", messages = listOf(message), needsYou = false, busy = false)
            val expected = c.getValue("expected").jsonObject
            assertEquals(c.str("name"), expected.str("kind"), preview.kind)
            assertEquals(c.str("name"), expected.str("text"), preview.text)
        }
    }
}
