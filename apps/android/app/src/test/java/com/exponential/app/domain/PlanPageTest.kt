package com.exponential.app.domain

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.long
import kotlinx.serialization.json.put
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/** [PlanPage] replays EVERY `*Cases` array of `fixtures/plan-page.json`,
 *  comparing the WHOLE expected value. */
class PlanPageTest {

    private val fixture: JsonObject by lazy {
        Json.parseToJsonElement(contractFixtureJson("plan-page.json")).jsonObject
    }

    private fun cases(key: String) = fixture.getValue(key).jsonArray.map { it.jsonObject }.also { assertTrue(it.isNotEmpty()) }

    private fun JsonObject.str(key: String): String? = this[key]?.takeUnless { it is JsonNull }?.jsonPrimitive?.content

    private fun num(value: JsonElement?): Float = value!!.jsonPrimitive.content.toFloat()

    private fun encode(block: PlanPage.Block): JsonObject = when (block) {
        is PlanPage.Block.Markdown -> buildJsonObject {
            put("kind", "markdown")
            put("text", block.text)
        }
        is PlanPage.Block.Section -> buildJsonObject {
            put("kind", "section")
            put("id", block.id)
            put("title", block.title)
        }
        is PlanPage.Block.Pictures -> buildJsonObject {
            put("kind", "pictures")
            put(
                "pictures",
                JsonArray(
                    block.pictures.map {
                        buildJsonObject {
                            put("alt", it.alt)
                            put("src", it.src)
                        }
                    },
                ),
            )
        }
    }

    private fun encode(counts: PlanPage.Counts) = buildJsonObject {
        put("sections", counts.sections)
        put("pictures", counts.pictures)
        put("questions", counts.questions)
    }

    @Test
    fun `constants match the fixture`() {
        assertEquals(fixture.str("route"), PlanPage.ROUTE)
        val copy = fixture.getValue("copy").jsonObject
        assertEquals(copy.str("title"), PlanPage.Copy.TITLE)
        assertEquals(copy.str("openPlan"), PlanPage.Copy.OPEN_PLAN)
        assertEquals(copy.str("openFull"), PlanPage.Copy.OPEN_FULL)
        assertEquals(copy.str("openChat"), PlanPage.Copy.OPEN_CHAT)
        assertEquals(copy.str("outline"), PlanPage.Copy.OUTLINE)
        assertEquals(copy.str("questions"), PlanPage.Copy.QUESTIONS)
        val counts = copy.getValue("counts").jsonObject
        fun words(key: String) = counts.getValue(key).jsonArray.map { it.jsonPrimitive.content }
        assertEquals(words("sections"), PlanPage.Copy.SECTIONS)
        assertEquals(words("pictures"), PlanPage.Copy.PICTURES)
        assertEquals(words("questions"), PlanPage.Copy.QUESTIONS_COUNT)
        val layout = fixture.getValue("layout").jsonObject
        assertEquals(num(layout["outlineWidth"]), PlanPage.Layout.OUTLINE_WIDTH)
        assertEquals(num(layout["columnMaxWidth"]), PlanPage.Layout.COLUMN_MAX_WIDTH)
        assertEquals(num(layout["railWidth"]), PlanPage.Layout.RAIL_WIDTH)
        assertEquals(num(layout.getValue("columnPadding").jsonObject["wide"]), PlanPage.Layout.COLUMN_PADDING_WIDE)
        assertEquals(num(layout.getValue("columnPadding").jsonObject["phone"]), PlanPage.Layout.COLUMN_PADDING_PHONE)
        assertEquals(num(layout["headerHeight"]), PlanPage.Layout.HEADER_HEIGHT)
        assertEquals(num(layout["blockGap"]), PlanPage.Layout.BLOCK_GAP)
        assertEquals(num(layout["outlineRowHeight"]), PlanPage.Layout.OUTLINE_ROW_HEIGHT)
        assertEquals(num(layout["outlineActiveBar"]), PlanPage.Layout.OUTLINE_ACTIVE_BAR)
        val mosaic = fixture.getValue("mosaic").jsonObject
        assertEquals(num(mosaic["gap"]), PlanPage.Mosaic.GAP)
        assertEquals(num(mosaic["largeFraction"]), PlanPage.Mosaic.LARGE_FRACTION)
        assertEquals(num(mosaic["aspect"]), PlanPage.Mosaic.ASPECT)
        assertEquals(num(mosaic["radius"]), PlanPage.Mosaic.RADIUS)
        assertEquals(
            mapOf(
                "page" to PlanPage.TestIds.PAGE, "outline" to PlanPage.TestIds.OUTLINE,
                "outlineRow" to PlanPage.TestIds.OUTLINE_ROW, "document" to PlanPage.TestIds.DOCUMENT,
                "rail" to PlanPage.TestIds.RAIL, "mosaic" to PlanPage.TestIds.MOSAIC,
                "carousel" to PlanPage.TestIds.CAROUSEL, "openPlan" to PlanPage.TestIds.OPEN_PLAN,
                "outlineToggle" to PlanPage.TestIds.OUTLINE_TOGGLE,
            ),
            fixture.getValue("testIds").jsonObject.mapValues { it.value.jsonPrimitive.content },
        )
        val specimen = fixture.getValue("specimen").jsonObject
        assertEquals(specimen.str("chatName"), PlanPage.Specimen.CHAT_NAME)
        assertEquals(specimen.str("activeSection"), PlanPage.Specimen.ACTIVE_SECTION)
    }

    @Test
    fun `replays every block case`() {
        for (case in cases("blockCases")) {
            val actual = JsonArray(PlanPage.blocks(case.str("markdown")!!).map(::encode))
            assertEquals(case.str("name"), case.getValue("expected"), actual)
        }
    }

    @Test
    fun `replays every outline case`() {
        for (case in cases("outlineCases")) {
            val actual = JsonArray(
                PlanPage.outline(case.str("markdown")!!).map {
                    buildJsonObject {
                        put("id", it.id)
                        put("title", it.title)
                        put("pictures", it.pictures)
                    }
                },
            )
            assertEquals(case.str("name"), case.getValue("expected"), actual)
        }
    }

    @Test
    fun `replays every count case`() {
        for (case in cases("countCases")) {
            assertEquals(case.str("name"), case.getValue("expected"), encode(PlanPage.counts(case["card"] as? JsonObject)))
        }
    }

    @Test
    fun `replays every counts line case`() {
        for (case in cases("countsLineCases")) {
            val counts = case.getValue("counts").jsonObject
            val actual = PlanPage.countsLine(
                PlanPage.Counts(
                    counts.getValue("sections").jsonPrimitive.int,
                    counts.getValue("pictures").jsonPrimitive.int,
                    counts.getValue("questions").jsonPrimitive.int,
                ),
            )
            assertEquals(case.str("expected"), actual)
        }
    }

    @Test
    fun `replays every mosaic case`() {
        for (case in cases("mosaicCases")) {
            val actual = JsonArray(
                PlanPage.mosaicTiles(case.getValue("count").jsonPrimitive.int).map {
                    buildJsonObject {
                        put("index", it.index)
                        put("size", it.size.wire)
                        put("more", it.more)
                    }
                },
            )
            assertEquals(case.getValue("expected"), actual)
        }
    }

    @Test
    fun `replays every target case`() {
        for (case in cases("targetCases")) {
            val messages = case.getValue("messages").jsonArray.map { it.jsonObject }.map {
                PlanPage.Message(
                    id = it.str("id")!!,
                    sessionId = it.str("sessionId")!!,
                    authorSessionId = it.str("authorSessionId"),
                    createdAt = it.getValue("createdAt").jsonPrimitive.long,
                    card = it["card"] as? JsonObject,
                )
            }
            val actual = PlanPage.planPageFor(case.str("sessionId")!!, messages)
            assertEquals(case.str("name"), case["expected"]?.takeUnless { it is JsonNull }?.jsonPrimitive?.content, actual)
        }
    }

    @Test
    fun `path fills the route`() {
        assertEquals("/t/acme/sessions/s1/plan/m1", PlanPage.path("acme", "s1", "m1"))
    }
}
