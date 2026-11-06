package com.exponential.app.data.push

import com.exponential.app.domain.contractFixtureJson
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

// #EXP-1639: the Live Update model, replayed off `live-activity.json`
// (TS `live-activity.test.ts`, same case names).
class LiveActivityTest {
    private val fixture: JsonObject = Json.parseToJsonElement(contractFixtureJson("live-activity.json")).jsonObject

    private fun JsonObject.str(key: String): String? =
        this[key]?.takeUnless { it is JsonNull }?.jsonPrimitive?.content

    private fun cases(key: String): List<JsonObject> =
        fixture.getValue(key).jsonArray.map { it.jsonObject }.also { assertTrue(key, it.isNotEmpty()) }

    private fun session(o: JsonObject) = LiveActivity.Session(
        id = o.str("id")!!,
        parentId = o.str("parentId"),
        userId = o.str("userId")!!,
        agentId = o.str("agentId"),
        startedByAgentId = o.str("startedByAgentId"),
        title = o.str("title"),
        sealedTitle = o.getValue("sealedTitle").jsonPrimitive.boolean,
        status = o.str("status")!!,
        busy = o.getValue("busy").jsonPrimitive.boolean,
        needsInput = o.getValue("needsInput").jsonPrimitive.boolean,
        blocked = o.getValue("blocked").jsonPrimitive.boolean,
        startedAt = o.str("startedAt"),
    )

    @Test
    fun `copy, segments and limits are the fixture's`() {
        val copy = fixture.getValue("copy").jsonObject
        assertEquals(copy.str("encryptedTitle"), LiveActivity.ENCRYPTED_TITLE)
        assertEquals(copy.str("prBadge"), LiveActivity.PR_BADGE)
        assertEquals(copy.getValue("phases").jsonObject.mapValues { it.value.jsonPrimitive.content }, LiveActivity.PHASES)
        assertEquals(fixture.getValue("segments").jsonArray.map { it.jsonPrimitive.content }, LiveActivity.SEGMENTS)
        assertEquals(fixture.getValue("maxActivities").jsonPrimitive.int, LiveActivity.MAX_ACTIVITIES)
        assertEquals(fixture.getValue("dismissMinutes").jsonPrimitive.int, LiveActivity.DISMISS_MINUTES)
        assertEquals(fixture.getValue("minUpdateSeconds").jsonPrimitive.int, LiveActivity.MIN_UPDATE_SECONDS)
    }

    @Test
    fun `followCases match the fixture`() {
        for (case in cases("followCases")) {
            val input = case.getValue("input").jsonObject
            val got = LiveActivity.follows(input.str("me")!!, input.getValue("sessions").jsonArray.map { session(it.jsonObject) })
            assertEquals(case.str("name"), case.getValue("expected").jsonArray.map { it.jsonPrimitive.content }, got)
        }
    }

    @Test
    fun `contentCases match the fixture`() {
        for (case in cases("contentCases")) {
            val input = case.getValue("input").jsonObject
            val e = case.getValue("expected").jsonObject
            val got = LiveActivity.content(
                session = session(input.getValue("session").jsonObject),
                deviceLabel = input.str("deviceLabel"),
                prNumber = input.str("prNumber")?.toInt(),
                askPreview = input.str("askPreview"),
                blockedLabel = input.str("blockedLabel"),
            )
            val expected = LiveActivity.Content(
                phase = e.str("phase")!!,
                title = e.str("title")!!,
                subtitle = e.str("subtitle")!!,
                detail = e.str("detail"),
                badge = e.str("badge")!!,
                segment = e.getValue("segment").jsonPrimitive.int,
            )
            assertEquals(case.str("name"), expected, got)
        }
    }

    @Test
    fun `the chip shows Needs you, a PR, or the chronometer`() {
        assertEquals("Needs you", LiveActivity.chipText("needs-you"))
        assertEquals("PR #12", LiveActivity.chipText("PR #12"))
        assertEquals(null, LiveActivity.chipText("timer"))
    }
}
