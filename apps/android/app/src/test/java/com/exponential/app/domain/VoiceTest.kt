package com.exponential.app.domain

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.booleanOrNull
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * NEW-67: Settings → Voice, locked ×4 (TS `voice.ts`, desktop
 * `domain::voice`, iOS `VoiceTests`) against `fixtures/voice.json`: every
 * `copy` key, the `testIds`, `shortcuts`, `defaults` and every case list but
 * the speech ones (shouldSpeak / spokenText / numbers are desktop-only).
 */
class VoiceTest {

    private val fixture: JsonObject by lazy {
        Json.parseToJsonElement(contractFixtureJson("voice.json")).jsonObject
    }

    private fun JsonObject.str(key: String): String? =
        this[key]?.takeUnless { it is JsonNull }?.jsonPrimitive?.content

    private fun cases(key: String): List<JsonObject> {
        val cases = fixture.getValue(key).jsonArray.map { it.jsonObject }
        assertTrue(key, cases.isNotEmpty())
        return cases
    }

    @Test
    fun `copy matches the fixture key by key`() {
        val copy = fixture.getValue("copy").jsonObject
        val mirrored = mapOf(
            "group" to Voice.Copy.GROUP,
            "voiceDevice" to Voice.Copy.VOICE_DEVICE,
            "voiceDeviceNone" to Voice.Copy.VOICE_DEVICE_NONE,
            "agent" to Voice.Copy.AGENT,
            "agentNone" to Voice.Copy.AGENT_NONE,
            "speakReplies" to Voice.Copy.SPEAK_REPLIES,
            "trayHeader" to Voice.Copy.TRAY_HEADER,
            "pushToTalk" to Voice.Copy.PUSH_TO_TALK,
            "shortcut" to Voice.Copy.SHORTCUT,
            "wakeWord" to Voice.Copy.WAKE_WORD,
            "wakePhrase" to Voice.Copy.WAKE_PHRASE,
            "wakePhrasePlaceholder" to Voice.Copy.WAKE_PHRASE_PLACEHOLDER,
            "downloading" to Voice.Copy.DOWNLOADING,
            "downloadFailed" to Voice.Copy.DOWNLOAD_FAILED,
            "unavailable" to Voice.Copy.UNAVAILABLE,
            "speaking" to Voice.Copy.SPEAKING,
            "stop" to Voice.Copy.STOP,
            "link" to Voice.Copy.LINK,
            "ellipsis" to Voice.Copy.ELLIPSIS,
        )
        // A key added to the fixture must land here too.
        assertEquals(copy.keys.associateWith { copy.str(it) }, mirrored)
    }

    @Test
    fun `test ids, shortcuts and maxChars match the fixture`() {
        val ids = fixture.getValue("testIds").jsonObject
        val mirrored = mapOf(
            "page" to Voice.TestIds.PAGE,
            "voiceDevice" to Voice.TestIds.VOICE_DEVICE,
            "agent" to Voice.TestIds.AGENT,
            "speakReplies" to Voice.TestIds.SPEAK_REPLIES,
            "pushToTalk" to Voice.TestIds.PUSH_TO_TALK,
            "shortcut" to Voice.TestIds.SHORTCUT,
            "wakeWord" to Voice.TestIds.WAKE_WORD,
            "wakePhrase" to Voice.TestIds.WAKE_PHRASE,
        )
        assertEquals(ids.keys.associateWith { ids.str(it) }, mirrored)
        val shortcuts = fixture.getValue("shortcuts").jsonArray.map { it.jsonObject }.map {
            Voice.Shortcut(value = it.str("value")!!, label = it.str("label")!!)
        }
        assertEquals(shortcuts, Voice.shortcuts)
        assertEquals(fixture.getValue("maxChars").jsonPrimitive.int, Voice.MAX_CHARS)
    }

    /** `defaults` decodes into the mirrored row (and `voice.get`'s shape). */
    @Test
    fun `defaults match the fixture`() {
        val decoded = Json.decodeFromJsonElement(Voice.Settings.serializer(), fixture.getValue("defaults"))
        assertEquals(Voice.defaults, decoded)
        assertTrue(Voice.shortcuts.any { it.value == Voice.defaults.pttShortcut })
    }

    @Test
    fun `every voiceDeviceOptions case matches the fixture`() {
        for (case in cases("voiceDeviceOptionsCases")) {
            val devices = case.getValue("devices").jsonArray.map { it.jsonObject }.map {
                Voice.DeviceLike(deviceId = it.str("deviceId")!!, name = it.str("name")!!, kind = it.str("kind")!!)
            }
            assertEquals(case.str("name"), options(case), Voice.voiceDeviceOptions(devices))
        }
    }

    @Test
    fun `every agentOptions case matches the fixture`() {
        for (case in cases("agentOptionsCases")) {
            val agents = case.getValue("agents").jsonArray.map { it.jsonObject }.map {
                Voice.AgentLike(id = it.str("id")!!, name = it.str("name")!!, team = it.str("team")!!)
            }
            assertEquals(case.str("name"), options(case), Voice.agentOptions(agents))
        }
    }

    @Test
    fun `every clearOnNone case matches the fixture`() {
        for (case in cases("clearOnNoneCases")) {
            val raw = case.getValue("change").jsonObject
            val change = Voice.Change(
                voiceDeviceId = if ("voiceDeviceId" in raw) Voice.Pick(raw.str("voiceDeviceId")) else null,
                agentId = if ("agentId" in raw) Voice.Pick(raw.str("agentId")) else null,
            )
            assertEquals(
                case.str("name"),
                settings(case.getValue("after").jsonObject),
                Voice.clearOnNone(settings(case.getValue("before").jsonObject), change),
            )
        }
    }

    @Test
    fun `every trayHeader case matches the fixture`() {
        for (case in cases("trayHeaderCases")) {
            assertEquals(case.str("header"), Voice.trayHeader(case.str("device")!!))
        }
    }

    @Test
    fun `every wakePhrasePlaceholder case matches the fixture`() {
        for (case in cases("wakePhrasePlaceholderCases")) {
            assertEquals(case.str("placeholder"), Voice.wakePhrasePlaceholder(case.str("agent")!!))
        }
    }

    /** None travels as an explicit null; an untouched field writes no key. */
    @Test
    fun `change encodes None as null and skips untouched fields`() {
        val json = Voice.Change(agentId = Voice.Pick(null), speakReplies = true).toJson()
        assertEquals(setOf("agentId", "speakReplies"), json.keys)
        assertEquals(JsonNull, json["agentId"])
        assertEquals(JsonPrimitive(true), json["speakReplies"])
        assertEquals(JsonObject(emptyMap()), Voice.Change().toJson())
    }

    private fun options(case: JsonObject): List<Voice.Option> =
        case.getValue("options").jsonArray.map { it.jsonObject }.map {
            Voice.Option(id = it.str("id"), label = it.str("label")!!)
        }

    /** A case's partial row over the defaults. */
    private fun settings(raw: JsonObject): Voice.Settings = Voice.defaults.copy(
        voiceDeviceId = raw.str("voiceDeviceId"),
        agentId = raw.str("agentId"),
        speakReplies = raw["speakReplies"]?.jsonPrimitive?.booleanOrNull ?: false,
        pushToTalk = raw["pushToTalk"]?.jsonPrimitive?.booleanOrNull ?: false,
        wakeWord = raw["wakeWord"]?.jsonPrimitive?.booleanOrNull ?: false,
    )
}
