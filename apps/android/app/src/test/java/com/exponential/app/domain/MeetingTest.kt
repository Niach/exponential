package com.exponential.app.domain

import java.io.File
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.long
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * NEW-57: [Meeting] replays `meeting.json` (copy, testIds, limits,
 * desktopAudioValues, desktopAudioOptionCases, appOptionCases, whoCases,
 * consentCases; #FEED-163 autoStop, autoStopCases, autoStoppedCases,
 * autoStopOptionCases, stopNoteCases) and locks the retention against `contract.json`.
 */
class MeetingTest {

    private val fixture: JsonObject =
        Json.parseToJsonElement(contractFixtureJson("meeting.json")).jsonObject

    private fun JsonObject.s(key: String): String = getValue(key).jsonPrimitive.content

    private fun cases(key: String): List<JsonObject> {
        val cases = fixture.getValue(key).jsonArray.map { it.jsonObject }
        assertTrue(key, cases.isNotEmpty())
        return cases
    }

    private fun apps(array: JsonArray): List<AudioApp> =
        array.map { it.jsonObject }.map { AudioApp(it.s("id"), it.s("name")) }

    @Test
    fun copyTestIdsLimitsAndValuesMatchTheFixture() {
        val copy = fixture.getValue("copy").jsonObject
        assertEquals(copy.keys, Meeting.Copy.all.keys)
        for ((key, value) in Meeting.Copy.all) assertEquals(key, copy.s(key), value)
        val testIds = fixture.getValue("testIds").jsonObject
        assertEquals(testIds.keys.associateWith { testIds.s(it) }, Meeting.TestIds.all)
        val limits = fixture.getValue("limits").jsonObject
        assertEquals(limits.getValue("apps").jsonPrimitive.int, Meeting.Limits.APPS)
        assertEquals(limits.getValue("appName").jsonPrimitive.int, Meeting.Limits.APP_NAME)
        val values = fixture.getValue("desktopAudioValues").jsonArray.map { it.jsonObject }
        assertEquals(values.map { it.s("value") to it.s("label") }, Meeting.DesktopAudio.entries.map { it.wire to it.label })
    }

    @Test
    fun retentionDaysMatchTheContract() {
        val relative = "packages/domain-contract/contract.json"
        val file = listOf("../../../$relative", "../../$relative", relative).map(::File).first { it.isFile }
        val contract = Json.parseToJsonElement(file.readText()).jsonObject
        assertEquals(
            contract.getValue("meeting").jsonObject.getValue("retentionDays").jsonPrimitive.int,
            Meeting.RETENTION_DAYS,
        )
    }

    @Test
    fun everyDesktopAudioOptionCaseReplays() {
        for (case in cases("desktopAudioOptionCases")) {
            val got = Meeting.desktopAudioOptions(Meeting.Capability.of(case.s("capability"))).map { it.wire }
            assertEquals(case.s("name"), case.getValue("expect").jsonArray.map { it.jsonPrimitive.content }, got)
        }
    }

    @Test
    fun everyAppOptionCaseReplays() {
        for (case in cases("appOptionCases")) {
            val expect = case.getValue("expect").jsonArray.map { it.jsonObject }
                .map { Meeting.AppOption(it.s("value"), it.s("label")) }
            val got = Meeting.appOptions(apps(case.getValue("live").jsonArray), apps(case.getValue("stored").jsonArray))
            assertEquals(case.s("name"), expect, got)
        }
    }

    @Test
    fun everyWhoCaseReplays() {
        for (case in cases("whoCases")) {
            val got = Meeting.who(
                Meeting.DesktopAudio.of(case.s("desktopAudio")),
                apps(case.getValue("apps").jsonArray),
                Meeting.Capability.of(case.s("capability")),
            )
            assertEquals(case.s("name"), case.s("expect"), got)
        }
    }

    @Test
    fun everyConsentCaseReplays() {
        for (case in cases("consentCases")) {
            val got = Meeting.consentBody(case.s("who"), case.s("agent"), case.getValue("days").jsonPrimitive.int)
            assertEquals(case.s("name"), case.s("expect"), got)
        }
    }

    private fun JsonObject.str(key: String): String? =
        get(key)?.takeIf { it !is JsonNull }?.jsonPrimitive?.content

    @Test
    fun autoStopMatchesTheFixture() {
        val autoStop = fixture.getValue("autoStop").jsonObject
        assertEquals(autoStop.getValue("afterMs").jsonPrimitive.long, Meeting.AutoStop.AFTER_MS)
        assertEquals(autoStop.getValue("defaultHours").jsonPrimitive.int, Meeting.AutoStop.DEFAULT_HOURS)
        assertEquals(autoStop.getValue("hoursValues").jsonArray.map { it.jsonPrimitive.int }, Meeting.AutoStop.HOURS_VALUES)
    }

    @Test
    fun everyAutoStopCaseReplays() {
        for (case in cases("autoStopCases")) {
            val nowMs = case.getValue("nowMs").jsonPrimitive.long
            val hours = case["hours"]?.jsonPrimitive?.int
            val got = if (hours == null) {
                Meeting.meetingExpired(case.str("startedAt"), nowMs)
            } else {
                Meeting.meetingExpired(case.str("startedAt"), nowMs, hours)
            }
            assertEquals(case.s("name"), case.getValue("expect").jsonPrimitive.content.toBoolean(), got)
        }
    }

    @Test
    fun everyAutoStoppedCaseReplays() {
        for (case in cases("autoStoppedCases")) {
            assertEquals(case.s("expect"), Meeting.autoStopped(case.getValue("hours").jsonPrimitive.int))
        }
    }

    @Test
    fun everyAutoStopOptionCaseReplays() {
        for (case in cases("autoStopOptionCases")) {
            val expect = case.getValue("expect").jsonArray.map { it.jsonObject }
                .map { Meeting.AutoStopOption(it.getValue("value").jsonPrimitive.int, it.s("label")) }
            assertEquals(case.s("name"), expect, Meeting.autoStopOptions())
        }
    }

    @Test
    fun everyStopNoteCaseReplays() {
        for (case in cases("stopNoteCases")) {
            val got = Meeting.stopNote(case.str("reason"), case.getValue("hours").jsonPrimitive.int)
            assertEquals(case.s("name"), case.str("expect"), got)
        }
    }

    @Test
    fun doctorDesktopAudioDecodesLeniently() {
        val doctor = parseDeviceDoctor(
            """{"items":[],"desktopAudio":{"capability":"per_app","apps":[{"id":"us.zoom.xos","name":"zoom.us"},{"id":1},"x"]}}""",
        )
        assertEquals(
            DesktopAudioDoctor(Meeting.Capability.PerApp, listOf(AudioApp("us.zoom.xos", "zoom.us"))),
            doctor?.desktopAudio,
        )
        assertEquals(DesktopAudioDoctor(), parseDeviceDoctor("""{"items":[]}""")?.desktopAudio)
        assertEquals(DesktopAudioDoctor(), parseDeviceDoctor("""{"items":[],"desktopAudio":"bad"}""")?.desktopAudio)
        assertEquals(
            Meeting.Capability.None,
            parseDeviceDoctor("""{"items":[],"desktopAudio":{"capability":"future"}}""")?.desktopAudio?.capability,
        )
    }

    @Test
    fun settingsCarryTheMeetingFields() {
        val row = Json { ignoreUnknownKeys = true }.decodeFromString(
            Voice.Settings.serializer(),
            """{"voiceDeviceId":"d","agentId":"a","desktopAudio":"include",""" +
                """"desktopAudioApps":[{"id":"z","name":"Zoom"}],"meetingStartedAt":"2026-10-28T10:00:00.000Z"}""",
        )
        assertEquals("include", row.desktopAudio)
        assertEquals(listOf(AudioApp("z", "Zoom")), row.desktopAudioApps)
        // A row from an older server reads off, no apps, no meeting.
        val legacy = Json.decodeFromString(Voice.Settings.serializer(), "{}")
        assertEquals("off", legacy.desktopAudio)
        assertEquals(null, legacy.meetingStartedAt)
        // #FEED-163: Stop after defaults to 4 h and round-trips.
        assertEquals(4, legacy.meetingAutoStopHours)
        val hours = Json { ignoreUnknownKeys = true }.decodeFromString(Voice.Settings.serializer(), """{"meetingAutoStopHours":8}""")
        assertEquals(8, hours.meetingAutoStopHours)
        assertEquals("""{"meetingAutoStopHours":1}""", Voice.Change(meetingAutoStopHours = 1).toJson().toString())
        assertEquals(1, Voice.clearOnNone(row, Voice.Change(meetingAutoStopHours = 1)).meetingAutoStopHours)
        // The wire input never carries meetingStartedAt.
        val change = Voice.Change(desktopAudio = "include", desktopAudioApps = listOf(AudioApp("z", "Zoom")))
        assertEquals(
            """{"desktopAudio":"include","desktopAudioApps":[{"id":"z","name":"Zoom"}]}""",
            change.toJson().toString(),
        )
        // Choosing None stops the meeting.
        assertEquals(null, Voice.clearOnNone(row, Voice.Change(agentId = Voice.Pick(null))).meetingStartedAt)
        assertEquals(row.meetingStartedAt, Voice.clearOnNone(row, Voice.Change(wakeWord = true)).meetingStartedAt)
    }
}
