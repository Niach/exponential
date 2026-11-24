package com.exponential.app.domain

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.put
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * #EXP-1859: agent-settings.json `voice.wake` replayed ×4 (TS
 * `agent-settings.test.ts`): every `listenersCases`, `rowCases`,
 * `toggleCases`, `clashCases` and `phraseClashCases` entry, plus the copy.
 */
class AgentSettingsWakeTest {

    private val wake: JsonObject by lazy {
        Json.parseToJsonElement(contractFixtureJson("agent-settings.json")).jsonObject
            .getValue("voice").jsonObject.getValue("wake").jsonObject
    }

    private fun JsonObject.str(key: String): String? =
        this[key]?.takeUnless { it is JsonNull }?.jsonPrimitive?.content

    private val agents: List<AgentWake.AgentLike> by lazy {
        wake.getValue("agents").jsonArray.map { it.jsonObject }.map {
            AgentWake.AgentLike(
                id = it.str("id")!!,
                role = it.str("role"),
                name = it.str("name")!!,
                voice = AgentVoiceLike.of(it.getValue("voice").jsonObject),
            )
        }
    }

    /** The fixture's base `settings` with the case's keys over it. */
    private fun input(case: JsonObject): AgentWake.Input {
        val merged = JsonObject(wake.getValue("settings").jsonObject + case.getValue("settings").jsonObject)
        val settings = AgentWake.SettingsLike(
            voiceDeviceId = merged.str("voiceDeviceId"),
            agentId = merged.str("agentId"),
            wakeWord = merged.getValue("wakeWord").jsonPrimitive.boolean,
            wakeAgentIds = merged.getValue("wakeAgentIds").jsonArray.map { it.jsonPrimitive.content },
            member = MemberVoiceLike(
                language = merged.str("language")!!,
                voiceFile = merged.str("voiceFile")!!,
                voiceTranscript = merged.str("voiceTranscript")!!,
                wakePhrase = merged.str("wakePhrase")!!,
            ),
        )
        return AgentWake.Input(settings, agents)
    }

    private fun nullable(text: String?): JsonElement = text?.let { JsonPrimitive(it) } ?: JsonNull

    private fun cases(key: String): List<JsonObject> =
        wake.getValue(key).jsonArray.map { it.jsonObject }.also { assertTrue(key, it.isNotEmpty()) }

    @Test
    fun `wake copy and limits are the fixture's`() {
        val copy = wake.getValue("copy").jsonObject
        assertEquals(copy.str("wakeWord"), AgentWake.Copy.WAKE_WORD)
        val errors = copy.getValue("errors").jsonObject
        assertEquals(errors.str("wakeTooShort"), AgentWake.Copy.ERROR_TOO_SHORT)
        assertEquals(errors.str("wakeClash"), AgentWake.Copy.ERROR_CLASH)
        assertEquals(errors.str("wakeCap"), AgentWake.Copy.ERROR_CAP)
        assertEquals(wake.str("maxListeners")!!.toInt(), AgentWake.MAX_LISTENERS)
        assertEquals(wake.str("maxIds")!!.toInt(), AgentWake.MAX_IDS)
        // The row shares the Assistant's Wake word label and the too-short copy.
        assertEquals(Voice.Copy.WAKE_WORD, AgentWake.Copy.WAKE_WORD)
        assertEquals(Voice.Copy.WAKE_PHRASE_TOO_SHORT, AgentWake.Copy.ERROR_TOO_SHORT)
    }

    @Test
    fun `wake maxEditRatio is talk-listen's`() {
        val talk = Json.parseToJsonElement(etalkFixtureJson("talk-listen.json")).jsonObject
        val ratio = talk.getValue("wake").jsonObject.getValue("constants").jsonObject.getValue("maxEditRatio").jsonPrimitive.content
        assertEquals(ratio.toDouble(), AgentWake.MAX_EDIT_RATIO, 0.0)
    }

    @Test
    fun `wake listenersCases`() {
        for (case in cases("listenersCases")) {
            val encoded = JsonArray(
                AgentWake.listeners(input(case)).map {
                    buildJsonObject {
                        put("agentId", it.agentId)
                        put("phrase", it.phrase)
                        put("language", it.language)
                    }
                },
            )
            assertEquals(case.str("name"), case.getValue("expected"), encoded)
        }
    }

    @Test
    fun `wake rowCases`() {
        for (case in cases("rowCases")) {
            val row = AgentWake.row(input(case), case.str("agentId")!!)
            val encoded = buildJsonObject {
                put("on", row.on)
                put("error", nullable(row.error))
            }
            assertEquals(case.str("name"), case.getValue("expected"), encoded)
        }
    }

    @Test
    fun `wake toggleCases`() {
        for (case in cases("toggleCases")) {
            val toggle = AgentWake.toggle(
                input(case),
                case.str("agentId")!!,
                case.getValue("on").jsonPrimitive.boolean,
            )
            val encoded = buildJsonObject {
                put("wakeWord", toggle.wakeWord)
                put("wakeAgentIds", JsonArray(toggle.wakeAgentIds.map { JsonPrimitive(it) }))
                put("error", nullable(toggle.error))
            }
            assertEquals(case.str("name"), case.getValue("expected"), encoded)
        }
    }

    @Test
    fun `wake clashCases`() {
        for (case in cases("clashCases")) {
            val a = case.str("a")!!
            val b = case.str("b")!!
            assertEquals("$a / $b", case.getValue("clash").jsonPrimitive.boolean, AgentWake.clash(a, b))
        }
    }

    @Test
    fun `wake phraseClashCases`() {
        val byId = agents.associateBy { it.id }
        for (case in cases("phraseClashCases")) {
            val clash = AgentWake.phraseClash(
                case.str("phrase")!!,
                case.str("agentId")!!,
                case.getValue("agents").jsonArray.map { byId.getValue(it.jsonPrimitive.content) },
            )
            assertEquals(case.str("name"), case.str("expected"), clash)
        }
    }

    @Test
    fun `wake letters fold accents and drop everything but letters and digits`() {
        assertEquals("heystudio", AgentWake.letters("Hey, Studio!"))
        assertEquals("hallojurgen", AgentWake.letters("Hallo Jürgen"))
        assertEquals("strasse2", AgentWake.letters("Straße 2"))
    }
}
