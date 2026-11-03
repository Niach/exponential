package com.exponential.app.domain

import at.exponential.ui.json.JsonValue
import java.io.File
import java.time.ZoneId
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

// VAPP-201: replays `packages/domain-contract/fixtures/team-zone.json`'s
// `zoneCases` and `resolveCases` (TS `team-zone.test.ts`, desktop
// `domain::team_zone`, iOS `TeamZoneTests`) and locks the copy and test id.
class TeamZoneTest {
    private fun read(relative: String): JsonValue {
        val candidates = listOf("../../../$relative", "../../$relative", relative)
        val file = candidates.map(::File).firstOrNull { it.isFile } ?: error("$relative not found from ${File(".").absolutePath}")
        return JsonValue.parse(file.readText())
    }

    private val fixture = read("packages/domain-contract/fixtures/team-zone.json")

    @Test
    fun `the copy and the test id match the fixture`() {
        assertEquals(TeamZone.Copy.ROW, fixture["copy"]!!["row"]!!.string)
        assertEquals(TeamZone.Copy.DEVICE, fixture["copy"]!!["device"]!!.string)
        assertEquals(TeamZone.TEST_ID_ROW, fixture["testIds"]!!["row"]!!.string)
    }

    @Test
    fun `zoneCases agree with the tz database`() {
        val cases = fixture["zoneCases"]!!.array!!
        assertTrue(cases.isNotEmpty())
        for (case in cases) {
            val zone = case["zone"]!!.string!!
            val ms = case["ms"]!!.number!!.toLong()
            assertEquals(zone, zone, TeamZone.resolveZone(zone))
            assertEquals("$zone at ${case["at"]!!.string}", case["offset"]!!.number!!.toInt(), TeamZone.zoneOffset(zone, ms))
        }
    }

    @Test
    fun `resolveCases`() {
        val cases = fixture["resolveCases"]!!.array!!
        assertTrue(cases.isNotEmpty())
        for (case in cases) {
            val teamZone = case["teamZone"]?.takeIf { it != JsonValue.Null }?.string
            val zone = case["zone"]?.takeIf { it != JsonValue.Null }?.string
            assertEquals(case["name"]!!.string, zone, TeamZone.resolveZone(teamZone))
        }
    }

    @Test
    fun `the host zone falls back to the device's`() {
        assertEquals(ZoneId.of("Europe/Vienna"), TeamZone.hostZoneId("Europe/Vienna"))
        assertEquals(ZoneId.systemDefault(), TeamZone.hostZoneId(null))
        assertEquals(ZoneId.systemDefault(), TeamZone.hostZoneId("Mars/Olympus_Mons"))
    }

    @Test
    fun dayIsTheZonesCalendarDay() {
        // Dogfood G: 2026-11-03T11:15Z is Nov 4 in Kiritimati (UTC+14).
        val ms = 1_793_704_500_000L
        assertEquals("2026-11-04", TeamZone.day(ms, TeamZone.zoneOffset("Pacific/Kiritimati", ms)))
        assertEquals("2026-11-03", TeamZone.day(ms, 0))
    }
}
