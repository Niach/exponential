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
        val markers = copy.getValue("markers").jsonObject.mapValues { it.value.jsonPrimitive.content }
        assertEquals(markers, TranscriptGrouping.Copy.MARKERS)
        assertEquals(copy.str("checkIn"), TranscriptGrouping.Copy.CHECK_IN)
        assertEquals(copy.str("checkIns"), TranscriptGrouping.Copy.CHECK_INS)
        assertEquals(copy.str("checkInTimeSeparator"), TranscriptGrouping.Copy.CHECK_IN_TIME_SEPARATOR)
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
        tail = o.getValue("tail").jsonPrimitive.boolean,
        gap = o.getValue("gap").jsonPrimitive.int,
    )

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

    @Test
    fun `every check-in case matches the fixture`() {
        val cases = fixture.getValue("checkInCases").jsonArray.map { it.jsonObject }
        assertTrue(cases.isNotEmpty())
        for (case in cases) {
            val kinds = case.getValue("kinds").jsonArray.map { k ->
                TranscriptGrouping.FeedRowRole.entries.first { it.wire == k.jsonPrimitive.content }
            }
            // #EXP-1873: JSON false / true / a number n.
            val expected = case.getValue("expected").jsonArray.map {
                val p = it.jsonPrimitive
                when (p.content) {
                    "false" -> TranscriptGrouping.FeedRowShown.Hidden
                    "true" -> TranscriptGrouping.FeedRowShown.Row
                    else -> TranscriptGrouping.FeedRowShown.CheckIn(p.int)
                }
            }
            assertEquals(case.str("name"), expected, TranscriptGrouping.feedRows(kinds))
        }
    }

    @Test
    fun `every check-in label case matches the fixture`() {
        val cases = fixture.getValue("checkInLabelCases").jsonArray.map { it.jsonObject }
        assertTrue(cases.isNotEmpty())
        for (case in cases) {
            assertEquals(
                case.str("name"),
                case.str("expected"),
                TranscriptGrouping.checkInLabel(case.getValue("count").jsonPrimitive.int, case.str("time")),
            )
        }
    }

    @Test
    fun `every blank case matches the fixture`() {
        val points = fixture.getValue("blank").jsonObject.getValue("codePoints").jsonArray.map { it.jsonPrimitive.content.toInt(16) }
        assertEquals(points, TranscriptGrouping.BLANK_CODE_POINTS)
        val cases = fixture.getValue("blankCases").jsonArray.map { it.jsonObject }
        assertTrue(cases.isNotEmpty())
        for (case in cases) {
            assertEquals(case.str("name"), case.getValue("expected").jsonPrimitive.boolean, TranscriptGrouping.isBlankText(case.str("text")))
        }
    }

    @Test
    fun `the check-in specimen matches the fixture`() {
        val rows = fixture.getValue("checkInSpecimen").jsonArray.map { it.jsonObject }.map { o ->
            TranscriptGrouping.CheckInSpecimenRow(
                TranscriptGrouping.FeedRowRole.entries.first { it.wire == o.str("kind") },
                o.str("time"),
                o.str("text"),
            )
        }
        assertEquals(rows, TranscriptGrouping.CHECK_IN_SPECIMEN)
        val agent = fixture.getValue("checkInSpecimenAgent").jsonObject
        assertEquals(
            TranscriptGrouping.CheckInSpecimenAgent(agent.str("name")!!, agent.str("icon"), agent.str("color")),
            TranscriptGrouping.CHECK_IN_SPECIMEN_AGENT,
        )
    }

    @Test
    fun `every notice case matches the fixture`() {
        val notices = fixture.getValue("notices").jsonObject.mapValues { it.value.jsonPrimitive.content }
        assertEquals(notices, TranscriptGrouping.NOTICES)
        // A notice reads its contract row's `done` caption.
        assertEquals("exponential_", DomainContract.expToolPrefix)
        for ((row, label) in notices) {
            val at = DomainContract.expToolNames.indexOf(row)
            assertTrue(row, at >= 0)
            assertEquals(row, DomainContract.expToolDone[at], label)
        }
        val cases = fixture.getValue("noticeCases").jsonArray.map { it.jsonObject }
        assertTrue(cases.isNotEmpty())
        for (case in cases) {
            assertEquals(case.str("name"), case.str("expected"), TranscriptGrouping.traceNotice(case.str("toolName")))
        }
    }
}
