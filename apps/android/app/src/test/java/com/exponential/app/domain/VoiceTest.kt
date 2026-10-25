package com.exponential.app.domain

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * NEW-53: `voice.json` ×4. The phones mirror `copy` + `testIds` and replay
 * every `assistantOptionsCases` entry (TS `voice.test.ts`).
 */
class VoiceTest {

    private val fixture: JsonObject by lazy {
        Json.parseToJsonElement(contractFixtureJson("voice.json")).jsonObject
    }

    private fun JsonObject.str(key: String): String? =
        this[key]?.takeUnless { it is JsonNull }?.jsonPrimitive?.content

    @Test
    fun `copy matches the fixture key by key`() {
        val copy = fixture.getValue("copy").jsonObject
        assertEquals(copy.str("group"), Voice.Copy.GROUP)
        assertEquals(copy.str("assistant"), Voice.Copy.ASSISTANT)
        assertEquals(copy.str("assistantNone"), Voice.Copy.ASSISTANT_NONE)
        assertEquals(copy.str("speakReplies"), Voice.Copy.SPEAK_REPLIES)
        assertEquals(copy.str("downloading"), Voice.Copy.DOWNLOADING)
        assertEquals(copy.str("downloadFailed"), Voice.Copy.DOWNLOAD_FAILED)
        assertEquals(copy.str("speaking"), Voice.Copy.SPEAKING)
        assertEquals(copy.str("stop"), Voice.Copy.STOP)
        assertEquals(copy.str("link"), Voice.Copy.LINK)
        assertEquals(copy.str("ellipsis"), Voice.Copy.ELLIPSIS)
        // A key added to the fixture must land here too.
        assertEquals(10, copy.size)
    }

    @Test
    fun `test ids and maxChars match the fixture`() {
        val ids = fixture.getValue("testIds").jsonObject
        assertEquals(ids.str("group"), Voice.TestIds.GROUP)
        assertEquals(ids.str("assistant"), Voice.TestIds.ASSISTANT)
        assertEquals(ids.str("speakReplies"), Voice.TestIds.SPEAK_REPLIES)
        assertEquals(3, ids.size)
        assertEquals(fixture.getValue("maxChars").jsonPrimitive.int, Voice.MAX_CHARS)
    }

    @Test
    fun `every assistantOptions case matches the fixture`() {
        val cases = fixture.getValue("assistantOptionsCases").jsonArray.map { it.jsonObject }
        assertTrue(cases.isNotEmpty())
        for (case in cases) {
            val agents = case.getValue("agents").jsonArray.map { it.jsonObject }.map {
                Voice.AgentLike(id = it.str("id")!!, name = it.str("name")!!, team = it.str("team")!!)
            }
            val expected = case.getValue("options").jsonArray.map { it.jsonObject }.map {
                Voice.AssistantOption(id = it.str("id"), label = it.str("label")!!)
            }
            assertEquals(case.str("name"), expected, Voice.assistantOptions(agents))
        }
    }
}
