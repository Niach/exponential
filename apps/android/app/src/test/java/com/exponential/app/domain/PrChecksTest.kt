package com.exponential.app.domain

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.long
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * #EXP-1611: `packages/domain-contract/fixtures/pr-checks.json`'s UI cases
 * (formatDuration, checksList, checksSummary, plus the
 * server cases' `aggregate`, copy and testIds) replayed against [PrChecks]
 * (TS `pr-checks.test.ts`, desktop + iOS mirrors).
 */
class PrChecksTest {

    private val fixture: JsonObject by lazy { Json.parseToJsonElement(contractFixtureJson("pr-checks.json")).jsonObject }

    private fun JsonObject.str(key: String): String? = this[key]?.takeIf { it !is JsonNull }?.jsonPrimitive?.contentOrNull

    private fun check(json: JsonElement): PrChecks.Check {
        val o = json.jsonObject
        return PrChecks.Check(
            name = o.str("name")!!,
            status = o.str("status")!!,
            conclusion = o.str("conclusion"),
            startedAt = o.str("startedAt"),
            completedAt = o.str("completedAt"),
            detailsUrl = o.str("detailsUrl"),
            required = o["required"]?.jsonPrimitive?.boolean ?: false,
        )
    }

    private fun checks(case: JsonObject) = case.getValue("checks").jsonArray.map(::check)

    private fun epoch(iso: String) = WireTimestamps.parseEpochMs(iso)!!

    @Test
    fun `copy and testIds are the fixture's`() {
        val copy = fixture.getValue("copy").jsonObject
        assertEquals(copy.str("checks"), PrChecks.Copy.CHECKS)
        val summary = copy.getValue("summary").jsonObject
        assertEquals(summary.str("failing"), PrChecks.Copy.SUMMARY_FAILING)
        assertEquals(summary.str("running"), PrChecks.Copy.SUMMARY_RUNNING)
        assertEquals(summary.str("passed"), PrChecks.Copy.SUMMARY_PASSED)
        val ids = fixture.getValue("testIds").jsonObject
        assertEquals(ids.str("checksMenu"), PrChecks.TEST_ID_CHECKS_MENU)
        assertEquals(ids.str("checkRow"), PrChecks.TEST_ID_CHECK_ROW)
    }

    @Test
    fun `every aggregate case matches the fixture`() {
        for (case in fixture.getValue("aggregate").jsonArray.map { it.jsonObject }) {
            assertEquals(case.str("name"), case.str("expected"), PrChecks.aggregate(checks(case)))
        }
    }

    @Test
    fun `every formatDuration case matches the fixture`() {
        for (case in fixture.getValue("formatDuration").jsonArray.map { it.jsonObject }) {
            val ms = case.getValue("ms").jsonPrimitive.long
            assertEquals("$ms", case.str("expected"), PrChecks.formatDuration(ms))
        }
    }

    @Test
    fun `every checksList case matches the fixture`() {
        val cases = fixture.getValue("checksList").jsonArray.map { it.jsonObject }
        assertTrue(cases.isNotEmpty())
        for (case in cases) {
            val expected = case.getValue("expected").jsonArray.map {
                val o = it.jsonObject
                listOf(o.str("name"), o.str("glyph"), o.str("tone"), o.str("duration"), o.str("url"))
            }
            val actual = PrChecks.checksList(checks(case), epoch(case.str("now")!!)).map {
                listOf(it.name, it.glyph.wire, it.tone.wire, it.duration, it.url)
            }
            assertEquals(case.str("name"), expected, actual)
        }
    }

    @Test
    fun `the specimen is the checksList specimen case`() {
        val specimen = fixture.getValue("specimen").jsonObject
        val listCase = fixture.getValue("checksList").jsonArray.first().jsonObject
        assertEquals(listCase.getValue("checks"), specimen.getValue("checks"))
        assertEquals(listCase.getValue("now"), specimen.getValue("now"))
        assertEquals(specimen.str("now"), PrChecks.SPECIMEN_NOW)
        assertEquals(specimen.getValue("checks").jsonArray.map(::check), PrChecks.SPECIMEN)
    }

    @Test
    fun `every checksSummary case matches the fixture`() {
        for (case in fixture.getValue("checksSummary").jsonArray.map { it.jsonObject }) {
            val expected = case["expected"]
            val actual = PrChecks.checksSummary(checks(case))
            if (expected == null || expected is JsonNull) {
                assertNull(case.str("name"), actual)
                continue
            }
            val o = expected.jsonObject
            assertEquals(
                case.str("name"),
                listOf(o.str("glyph"), o.str("tone"), o.str("value")),
                actual?.let { listOf(it.glyph.wire, it.tone.wire, it.value) },
            )
        }
    }
}
