package com.exponential.app.domain

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.long
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * EXP-1248: THE list item, locked x4 against `list-item.json` (web
 * `session-row-caption.test.ts`, desktop `domain::list_item` /
 * `domain::session_row`, iOS `ListItemTests`).
 */
class ListItemTest {

    private val fixture: JsonObject =
        Json.parseToJsonElement(contractFixtureJson("list-item.json")).jsonObject

    private fun JsonObject.string(key: String): String? = get(key)?.jsonPrimitive?.contentOrNull

    private fun state(wire: String): CodingSessionDisplayState = when (wire) {
        "working" -> CodingSessionDisplayState.Working
        "needs_input" -> CodingSessionDisplayState.NeedsInput
        "review" -> CodingSessionDisplayState.Review
        "done" -> CodingSessionDisplayState.Done
        else -> error("unknown state $wire")
    }

    private fun tone(wire: String): SessionStatusTone = when (wire) {
        "muted" -> SessionStatusTone.Muted
        "amber" -> SessionStatusTone.Amber
        "emerald" -> SessionStatusTone.Emerald
        "sky" -> SessionStatusTone.Sky
        else -> error("unknown tone $wire")
    }

    @Test
    fun `list_elapsed matches the fixture`() {
        val cases = fixture.getValue("elapsed").jsonArray.map { it.jsonObject }
        assertTrue(cases.isNotEmpty())
        for (case in cases) {
            assertEquals(case.string("name"), case.string("expected"), listElapsed(case.getValue("ms").jsonPrimitive.long))
        }
    }

    @Test
    fun `session_row_caption matches the fixture`() {
        val cases = fixture.getValue("captions").jsonArray.map { it.jsonObject }
        assertTrue(cases.isNotEmpty())
        for (case in cases) {
            val expected = case.getValue("expected").jsonObject
            val actual = sessionRowCaption(
                ended = case.getValue("ended").jsonPrimitive.boolean,
                paused = case.getValue("paused").jsonPrimitive.boolean,
                state = state(case.string("state")!!),
                device = case.string("device"),
                startedAt = case.string("startedAt"),
                updatedAt = case.string("updatedAt"),
                endedAt = case.string("endedAt"),
                blockedLabel = case.string("blockedLabel"),
                nowMs = WireTimestamps.parseEpochMs(case.string("now")!!)!!,
            )
            assertEquals(
                case.string("name"),
                SessionRowCaption(expected.string("text")!!, tone(expected.string("tone")!!)),
                actual,
            )
        }
    }

    @Test
    fun `paints every live state in session-display json's statusTone`() {
        val display = Json.parseToJsonElement(contractFixtureJson("session-display.json")).jsonObject
        for (element in display.getValue("cases").jsonArray) {
            val case = element.jsonObject
            if (case.string("status") == "ended") continue
            val caption = sessionRowCaption(
                ended = false,
                paused = false,
                state = state(case.string("state")!!),
                device = "mint",
                startedAt = null,
                updatedAt = null,
                endedAt = null,
                nowMs = 0,
            )
            assertEquals(case.string("name"), tone(case.string("statusTone")!!), caption.tone)
        }
    }

    @Test
    fun `pr_node states match the fixture`() {
        val cases = fixture.getValue("prNodes").jsonArray.map { it.jsonObject }
        assertEquals(3, cases.size)
        for (case in cases) {
            val node = PrNodeState.fromWire(case.string("state")!!)!!
            val ring = if (case.string("ring") == "muted") PrNodeRing.Muted else PrNodeRing.Emerald
            assertEquals(case.string("name"), ring, node.ring)
            assertEquals(case.string("name"), case.getValue("filled").jsonPrimitive.boolean, node.filled)
        }
    }

    @Test
    fun `the geometry matches the fixture`() {
        val geometry = fixture.getValue("geometry").jsonObject
        fun dp(key: String) = geometry.getValue(key).jsonPrimitive.int
        assertEquals(dp("base"), ListItem.BASE_DP)
        assertEquals(dp("indent"), ListItem.INDENT_DP)
        assertEquals(dp("mark"), ListItem.MARK_DP)
        assertEquals(dp("gap"), ListItem.GAP_DP)
        assertEquals(dp("small"), ListItem.SMALL_DP)
        assertEquals(dp("big"), ListItem.BIG_DP)
        assertEquals(dp("prRow"), ListItem.PR_ROW_DP)
        assertEquals(dp("prRowPhone"), ListItem.PR_ROW_PHONE_DP)
        assertEquals(dp("prNode"), ListItem.PR_NODE_DP)
        assertEquals(dp("rail"), ListItem.RAIL_DP)
        // The tree connector measures from the same base and indent.
        assertEquals(TreeGuides.BASE_DP, ListItem.BASE_DP)
        assertEquals(TreeGuides.INDENT_DP, ListItem.INDENT_DP)
        assertEquals(12 + 14 * 2, ListItem.leadX(2))
    }
}
