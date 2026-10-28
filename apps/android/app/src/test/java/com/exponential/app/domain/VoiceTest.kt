package com.exponential.app.domain

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.boolean
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
 * `copy` key, the `testIds`, `shortcutGrammar`, `languages`, `limits`, `defaults` and every case list but
 * the speech and tray ones (shouldSpeak / spokenText / numbers / trayMenu / railVoice are desktop-only);
 * S-3: `shortcutParseCases`, `captureCases`, `wakeDetailCases`.
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
            "language" to Voice.Copy.LANGUAGE,
            "languageNote" to Voice.Copy.LANGUAGE_NOTE,
            "voiceFile" to Voice.Copy.VOICE_FILE,
            "voiceFilePlaceholder" to Voice.Copy.VOICE_FILE_PLACEHOLDER,
            "voiceTranscript" to Voice.Copy.VOICE_TRANSCRIPT,
            "voiceTranscriptPlaceholder" to Voice.Copy.VOICE_TRANSCRIPT_PLACEHOLDER,
            "trayHeader" to Voice.Copy.TRAY_HEADER,
            "pushToTalk" to Voice.Copy.PUSH_TO_TALK,
            "wakeWord" to Voice.Copy.WAKE_WORD,
            "wakePhrase" to Voice.Copy.WAKE_PHRASE,
            "wakePhrasePlaceholder" to Voice.Copy.WAKE_PHRASE_PLACEHOLDER,
            "wakePhraseTooShort" to Voice.Copy.WAKE_PHRASE_TOO_SHORT,
            "trayOpen" to Voice.Copy.TRAY_OPEN,
            "trayCall" to Voice.Copy.TRAY_CALL,
            "trayMute" to Voice.Copy.TRAY_MUTE,
            "traySettings" to Voice.Copy.TRAY_SETTINGS,
            "trayQuit" to Voice.Copy.TRAY_QUIT,
            "downloading" to Voice.Copy.DOWNLOADING,
            "downloadingProgress" to Voice.Copy.DOWNLOADING_PROGRESS,
            "downloadFailed" to Voice.Copy.DOWNLOAD_FAILED,
            "unavailable" to Voice.Copy.UNAVAILABLE,
            "speaking" to Voice.Copy.SPEAKING,
            "stop" to Voice.Copy.STOP,
            "link" to Voice.Copy.LINK,
            "ellipsis" to Voice.Copy.ELLIPSIS,
            "shortcutNone" to Voice.Copy.SHORTCUT_NONE,
            "shortcutRecord" to Voice.Copy.SHORTCUT_RECORD,
            "shortcutClear" to Voice.Copy.SHORTCUT_CLEAR,
            "trayEndCall" to Voice.Copy.TRAY_END_CALL,
        )
        // A key added to the fixture must land here too.
        assertEquals(copy.keys.associateWith { copy.str(it) }, mirrored)
    }

    @Test
    fun `test ids, the shortcut grammar and maxChars match the fixture`() {
        val ids = fixture.getValue("testIds").jsonObject
        val mirrored = mapOf(
            "page" to Voice.TestIds.PAGE,
            "voiceDevice" to Voice.TestIds.VOICE_DEVICE,
            "agent" to Voice.TestIds.AGENT,
            "language" to Voice.TestIds.LANGUAGE,
            "voiceFile" to Voice.TestIds.VOICE_FILE,
            "voiceTranscript" to Voice.TestIds.VOICE_TRANSCRIPT,
            "shortcut" to Voice.TestIds.SHORTCUT,
            "wakeWord" to Voice.TestIds.WAKE_WORD,
            "wakePhrase" to Voice.TestIds.WAKE_PHRASE,
            "shortcutClear" to Voice.TestIds.SHORTCUT_CLEAR,
        )
        assertEquals(ids.keys.associateWith { ids.str(it) }, mirrored)
        val grammar = fixture.getValue("shortcutGrammar").jsonObject
        val g = Voice.ShortcutGrammar
        fun list(key: String) = grammar.getValue(key).jsonArray.map { it.jsonPrimitive.content }
        fun range(key: String) = grammar.getValue(key).jsonObject.let { it.getValue("min").jsonPrimitive.int to it.getValue("max").jsonPrimitive.int }
        assertEquals(list("mods"), g.MODS)
        assertEquals(list("named"), g.NAMED)
        assertEquals(list("bareKeys"), g.BARE_KEYS)
        assertEquals(range("mouse"), g.MOUSE_MIN to g.MOUSE_MAX)
        assertEquals(range("function"), g.FUNCTION_MIN to g.FUNCTION_MAX)
        assertEquals(grammar.getValue("maxLength").jsonPrimitive.int, g.MAX_LENGTH)
        assertEquals(grammar.str("separator"), g.SEPARATOR)
        assertEquals(grammar.getValue("labels").jsonObject.let { l -> l.keys.associateWith { l.str(it) } }, g.LABELS)
        val ptt = fixture.getValue("pttCopy").jsonObject
        assertEquals(ptt.str("shortcutNeedsModifier"), Voice.PttCopy.SHORTCUT_NEEDS_MODIFIER)
        val wake = ptt.getValue("wakeDetail").jsonObject
        assertEquals(wake.keys.associateWith { wake.str(it) }, Voice.WakeState.entries.associate { it.wire to it.detail })
        assertEquals(fixture.getValue("maxChars").jsonPrimitive.int, Voice.MAX_CHARS)
    }

    @Test
    fun `languages and limits match the fixture`() {
        val languages = fixture.getValue("languages").jsonArray.map { it.jsonObject }.map {
            Voice.Language(value = it.str("value")!!, label = it.str("label")!!)
        }
        assertEquals(languages, Voice.languages)
        val limits = fixture.getValue("limits").jsonObject
        assertEquals(setOf("voiceFile", "voiceTranscript", "wakePhrase", "wakePhraseMinLetters"), limits.keys)
        assertEquals(limits.getValue("wakePhrase").jsonPrimitive.int, Voice.Limits.WAKE_PHRASE)
        assertEquals(limits.getValue("voiceFile").jsonPrimitive.int, Voice.Limits.VOICE_FILE)
        assertEquals(limits.getValue("voiceTranscript").jsonPrimitive.int, Voice.Limits.VOICE_TRANSCRIPT)
        assertEquals(limits.getValue("wakePhraseMinLetters").jsonPrimitive.int, Voice.Limits.WAKE_PHRASE_MIN_LETTERS)
    }

    @Test
    fun `every languageNote case matches the fixture`() {
        for (case in cases("languageNoteCases")) {
            assertEquals(case.str("name"), case.str("note"), Voice.languageNote(case.str("language")!!, case.str("device")))
        }
    }

    @Test
    fun `every languageLabel case matches the fixture`() {
        for (case in cases("languageLabelCases")) {
            assertEquals(case.str("name"), case.str("label"), Voice.languageLabel(case.str("language")!!, case.str("device")))
        }
    }

    /** `defaults` decodes into the mirrored row (and `voice.get`'s shape). */
    @Test
    fun `defaults match the fixture`() {
        val decoded = Json.decodeFromJsonElement(Voice.Settings.serializer(), fixture.getValue("defaults"))
        assertEquals(Voice.defaults, decoded)
        assertEquals("", Voice.defaults.pttShortcut)
        assertTrue(Voice.languages.any { it.value == Voice.defaults.language })
        // A row from an older server without the #NEW-73 keys reads the defaults.
        val legacy = Json.decodeFromString(Voice.Settings.serializer(), """{"wakePhrase":"Hi"}""")
        assertEquals(Voice.defaults.copy(wakePhrase = "Hi"), legacy)
        // S-3: an older server's speakReplies / pushToTalk are tolerated and ignored.
        val lenient = Json { ignoreUnknownKeys = true }
        val old = lenient.decodeFromString(Voice.Settings.serializer(), """{"speakReplies":true,"pushToTalk":true,"pttShortcut":"ctrl+space"}""")
        assertEquals(Voice.defaults.copy(pttShortcut = "ctrl+space"), old)
    }

    @Test
    fun `every shortcutParse case matches the fixture`() {
        for (case in cases("shortcutParseCases")) {
            val input = case.str("in")!!
            val value = Voice.shortcutCanonical(input)
            assertEquals(input, case.str("value"), value)
            if (value != null) assertEquals(input, case.str("label"), Voice.shortcutLabel(value))
        }
    }

    @Test
    fun `every capture case matches the fixture`() {
        for (case in cases("captureCases")) {
            val name = case.str("name")
            var state = Voice.captureStart(case.str("before")!!)
            for (raw in case.getValue("events").jsonArray.map { it.jsonObject }) {
                val event = when (raw.str("t")) {
                    "down" -> Voice.CaptureEvent.Down(raw.str("key")!!)
                    "up" -> Voice.CaptureEvent.Up(raw.str("key")!!)
                    "mouse" -> Voice.CaptureEvent.Mouse(raw.getValue("button").jsonPrimitive.int)
                    else -> error("$name: $raw")
                }
                state = Voice.captureStep(state, event)
            }
            val result = case.getValue("result").jsonObject
            assertEquals(name, result.str("status"), state.status.wire)
            assertEquals(name, result.str("value"), state.value)
            assertEquals(name, result.str("pending"), Voice.capturePending(state))
            assertEquals(name, result.str("error"), state.error)
        }
    }

    @Test
    fun `every wakeDetail case matches the fixture`() {
        for (case in cases("wakeDetailCases")) {
            val c = case.getValue("context").jsonObject
            val context = Voice.WakeDetailContext(
                wakeWord = c.getValue("wakeWord").jsonPrimitive.boolean,
                agent = c.getValue("agent").jsonPrimitive.boolean,
                online = c.getValue("online").jsonPrimitive.boolean,
                device = c.str("device")!!,
                state = c.str("state")?.let { s -> Voice.WakeState.entries.single { it.wire == s } },
            )
            assertEquals(case.str("name"), case.str("detail"), Voice.wakeDetail(context))
        }
    }

    /** S-3: a phone reads the voice device's doctor row `wake` verbatim; no agent / offline win over it. */
    @Test
    fun `the wake row reads the voice device's doctor row`() {
        val on = Voice.WakeDetailContext(wakeWord = true, agent = true, online = true, device = "mint", state = null)
        val doctor = DeviceDoctor(items = listOf(DeviceDoctorItem(key = "wake", group = "voice", state = "off", detail = "A call is using the microphone")))
        assertEquals("A call is using the microphone", Voice.wakeRowDetail(doctor, on))
        assertEquals("Starting…", Voice.wakeRowDetail(null, on))
        assertEquals("Starting…", Voice.wakeRowDetail(DeviceDoctor(items = listOf(DeviceDoctorItem(key = "wake", group = "agents", state = "ok", detail = "x"))), on))
        assertEquals("mint is offline", Voice.wakeRowDetail(doctor, on.copy(online = false)))
        assertEquals("Pick an agent", Voice.wakeRowDetail(doctor, on.copy(agent = false)))
        assertEquals(null, Voice.wakeRowDetail(doctor, on.copy(wakeWord = false)))
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

    @Test
    fun `every wakePhraseError case matches the fixture`() {
        for (case in cases("wakePhraseErrorCases")) {
            assertEquals(case.str("in"), case.str("error"), Voice.wakePhraseError(case.str("in")!!))
        }
    }

    /** #NEW-75: THE input rule (UTF-16 units, never a split surrogate pair). */
    @Test
    fun `every voiceTextInput case matches the fixture`() {
        for (case in cases("voiceTextInputCases")) {
            val max = case.getValue("max").jsonPrimitive.int
            assertEquals(case.str("in"), case.str("out"), Voice.textInput(case.str("in")!!, max))
        }
    }

    /** None travels as an explicit null; an untouched field writes no key. */
    @Test
    fun `change encodes None as null and skips untouched fields`() {
        val json = Voice.Change(agentId = Voice.Pick(null), wakeWord = true).toJson()
        assertEquals(setOf("agentId", "wakeWord"), json.keys)
        assertEquals(JsonNull, json["agentId"])
        assertEquals(JsonPrimitive(true), json["wakeWord"])
        assertEquals(JsonObject(emptyMap()), Voice.Change().toJson())
        val typed = Voice.Change(language = "de", voiceFile = "/v.wav", voiceTranscript = "Hallo").toJson()
        assertEquals(setOf("language", "voiceFile", "voiceTranscript"), typed.keys)
        assertEquals(JsonPrimitive("de"), typed["language"])
    }

    private fun options(case: JsonObject): List<Voice.Option> =
        case.getValue("options").jsonArray.map { it.jsonObject }.map {
            Voice.Option(id = it.str("id"), label = it.str("label")!!)
        }

    /** A case's partial row over the defaults. */
    private fun settings(raw: JsonObject): Voice.Settings = Voice.defaults.copy(
        voiceDeviceId = raw.str("voiceDeviceId"),
        agentId = raw.str("agentId"),
        wakeWord = raw["wakeWord"]?.jsonPrimitive?.booleanOrNull ?: false,
    )
}
