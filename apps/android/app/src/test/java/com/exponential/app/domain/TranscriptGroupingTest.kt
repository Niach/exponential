package com.exponential.app.domain

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.float
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * #EXP-1736: [TranscriptGrouping] replays `fixtures/transcript-grouping.json`
 * (TS `transcript-grouping.ts`), comparing the WHOLE expected array.
 */
class TranscriptGroupingTest {

    private val fixture: JsonObject by lazy {
        Json.parseToJsonElement(contractFixtureJson("transcript-grouping.json")).jsonObject
    }

    private fun JsonObject.str(key: String): String? =
        this[key]?.takeUnless { it is JsonNull }?.jsonPrimitive?.content

    @Test
    fun `layout and copy match the fixture`() {
        val layout = fixture.getValue("layout").jsonObject
        assertEquals(layout.getValue("avatar").jsonPrimitive.int, TranscriptGrouping.Layout.AVATAR)
        assertEquals(layout.getValue("avatarGap").jsonPrimitive.int, TranscriptGrouping.Layout.AVATAR_GAP)
        assertEquals(layout.getValue("gapInGroup").jsonPrimitive.int, TranscriptGrouping.Layout.GAP_IN_GROUP)
        assertEquals(layout.getValue("gapBetween").jsonPrimitive.int, TranscriptGrouping.Layout.GAP_BETWEEN)
        val maxWidth = layout.getValue("maxWidth").jsonObject
        assertEquals(maxWidth.getValue("phone").jsonPrimitive.float, TranscriptGrouping.Layout.MAX_WIDTH_PHONE, 0f)
        assertEquals(maxWidth.getValue("phoneIn").jsonPrimitive.float, TranscriptGrouping.Layout.MAX_WIDTH_PHONE_IN, 0f)
        assertEquals(maxWidth.getValue("wide").jsonPrimitive.float, TranscriptGrouping.Layout.MAX_WIDTH_WIDE, 0f)
        assertEquals(maxWidth.getValue("own").jsonPrimitive.float, TranscriptGrouping.Layout.MAX_WIDTH_OWN, 0f)
        assertEquals(layout.getValue("radius").jsonPrimitive.int, TranscriptGrouping.Layout.RADIUS)
        assertEquals(layout.getValue("nameSize").jsonPrimitive.int, TranscriptGrouping.Layout.NAME_SIZE)
        val copy = fixture.getValue("copy").jsonObject
        assertEquals(copy.str("formerMember"), TranscriptGrouping.Copy.FORMER_MEMBER)
        assertEquals(copy.str("linkChevron"), TranscriptGrouping.Copy.LINK_CHEVRON)
        assertEquals(copy.str("showMore"), TranscriptGrouping.Copy.SHOW_MORE)
        assertEquals(copy.str("showLess"), TranscriptGrouping.Copy.SHOW_LESS)
        val fold = fixture.getValue("fold").jsonObject
        assertEquals(fold.getValue("lines").jsonPrimitive.int, TranscriptGrouping.Fold.LINES)
        assertEquals(fold.getValue("chars").jsonPrimitive.int, TranscriptGrouping.Fold.CHARS)
        val markers = copy.getValue("markers").jsonObject.mapValues { it.value.jsonPrimitive.content }
        assertEquals(markers, TranscriptGrouping.Copy.MARKERS)
    }

    private fun input(o: JsonObject): TranscriptGrouping.Input {
        val chat = o.getValue("chat").jsonObject
        return TranscriptGrouping.Input(
            viewerId = o.str("viewerId"),
            chat = TranscriptGrouping.Chat(
                sessionId = chat.str("sessionId")!!,
                title = chat.str("title")!!,
                agentId = chat.str("agentId"),
                originSessionId = chat.str("originSessionId"),
                fromStart = chat["fromStart"]?.takeUnless { it is JsonNull }?.jsonPrimitive?.boolean ?: true,
            ),
            members = o.getValue("members").jsonArray.map {
                TranscriptGrouping.Named(it.jsonObject.str("id")!!, it.jsonObject.str("name")!!)
            },
            agents = o.getValue("agents").jsonArray.map {
                TranscriptGrouping.Named(it.jsonObject.str("id")!!, it.jsonObject.str("name")!!)
            },
            sessions = o.getValue("sessions").jsonArray.map {
                val s = it.jsonObject
                TranscriptGrouping.SessionRef(s.str("id")!!, s.str("title")!!, s.str("agentId"))
            },
            items = o.getValue("items").jsonArray.map {
                val i = it.jsonObject
                TranscriptGrouping.Item(
                    id = i.str("id")!!,
                    kind = TranscriptGrouping.ItemKind.entries.first { k -> k.wire == i.str("kind") },
                    authorKind = i.str("authorKind"),
                    authorId = i.str("authorId"),
                    authorSessionId = i.str("authorSessionId"),
                )
            },
        )
    }

    private fun row(o: JsonObject): TranscriptGrouping.RowLayout = TranscriptGrouping.RowLayout(
        id = o.str("id")!!,
        side = TranscriptGrouping.Side.entries.first { it.wire == o.str("side") },
        sender = (o["sender"]?.takeUnless { it is JsonNull })?.jsonObject?.let { s ->
            TranscriptGrouping.Sender(
                kind = TranscriptGrouping.SenderKind.entries.first { it.wire == s.str("kind") },
                id = s.str("id")!!,
                name = s.str("name")!!,
                link = s.str("link"),
            )
        },
        avatar = o.getValue("avatar").jsonPrimitive.boolean,
        name = o.getValue("name").jsonPrimitive.boolean,
        gap = o.getValue("gap").jsonPrimitive.int,
    )

    @Test
    fun `every fold case matches the fixture`() {
        val cases = fixture.getValue("foldCases").jsonArray.map { it.jsonObject }
        assertTrue(cases.isNotEmpty())
        for (case in cases) {
            assertEquals(
                case.str("name"),
                case.getValue("expected").jsonPrimitive.boolean,
                TranscriptGrouping.messageFolds(case.str("text")!!),
            )
        }
    }

    @Test
    fun `every case matches the fixture`() {
        val cases = fixture.getValue("cases").jsonArray.map { it.jsonObject }
        assertTrue(cases.isNotEmpty())
        for (case in cases) {
            val expected = case.getValue("expected").jsonArray.map { row(it.jsonObject) }
            assertEquals(
                case.str("name"),
                expected,
                TranscriptGrouping.transcriptLayout(input(case.getValue("input").jsonObject)),
            )
        }
    }

    @Test
    fun `every input case matches the fixture`() {
        val cases = fixture.getValue("inputCases").jsonArray.map { it.jsonObject }
        assertTrue(cases.isNotEmpty())
        for (case in cases) {
            val e = case["expected"]?.takeUnless { it is JsonNull }?.jsonObject
            val expected = e?.let {
                TranscriptGrouping.CrossSessionInput(
                    kind = TranscriptGrouping.InputKind.entries.first { k -> k.wire == it.str("kind") },
                    name = it.str("name")!!,
                    sessionId = it.str("sessionId")!!,
                    body = it.str("body")!!,
                )
            }
            assertEquals(case.str("name"), expected, TranscriptGrouping.crossSessionInput(case.str("text")!!))
        }
    }

    @Test
    fun `every feed input case matches the fixture`() {
        val cases = fixture.getValue("feedInputCases").jsonArray.map { it.jsonObject }
        assertTrue(cases.isNotEmpty())
        for (case in cases) {
            val e = case["expected"]?.takeUnless { it is JsonNull }?.jsonObject
            val expected = e?.let {
                when (it.str("kind")) {
                    "marker" -> TranscriptGrouping.FeedInput.Marker(it.str("key")!!, it.str("label")!!)
                    else -> TranscriptGrouping.FeedInput.Relay(
                        name = it.str("name")!!,
                        sessionId = it.str("sessionId")!!,
                        body = it.str("body")!!,
                        report = it.getValue("report").jsonPrimitive.boolean,
                    )
                }
            }
            assertEquals(case.str("name"), expected, TranscriptGrouping.feedInput(case.str("text")!!))
        }
    }

    @Test
    fun `every marker case matches the fixture`() {
        val cases = fixture.getValue("markerCases").jsonArray.map { it.jsonObject }
        assertTrue(cases.isNotEmpty())
        for (case in cases) {
            val kinds = case.getValue("kinds").jsonArray.map { k ->
                TranscriptGrouping.FeedRowKind.entries.first { it.wire == k.jsonPrimitive.content }
            }
            val expected = case.getValue("expected").jsonArray.map { it.jsonPrimitive.boolean }
            assertEquals(case.str("name"), expected, TranscriptGrouping.markersShown(kinds))
        }
    }
}
