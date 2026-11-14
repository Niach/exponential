package com.exponential.app.data.db

import androidx.sqlite.db.SupportSQLiteDatabase
import com.exponential.app.data.api.AgentVoicePatch
import com.exponential.app.data.api.UpdateAgentPatch
import com.exponential.app.data.api.updateAgentInput
import com.exponential.app.data.api.uploadRouteErrorMessage
import com.exponential.app.domain.AgentVoiceLike
import java.lang.reflect.Proxy
import kotlinx.serialization.json.Json
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Test

/**
 * #EXP-1847 (agent-settings.json `voice`): v129 adds `agents.voice` and
 * refetches ONLY the agents shape; the synced row decodes it tolerant of
 * absence; `agents.update {voice}` sends only the touched keys (blank = null).
 */
class AgentSettingsVoiceWireTest {
    private fun recording(statements: MutableList<String>): SupportSQLiteDatabase =
        Proxy.newProxyInstance(
            SupportSQLiteDatabase::class.java.classLoader,
            arrayOf(SupportSQLiteDatabase::class.java),
        ) { _, method, args ->
            if (method.name == "execSQL") {
                statements += args!![0] as String
                null
            } else {
                error("unexpected ${method.name}")
            }
        } as SupportSQLiteDatabase

    private val json = Json { ignoreUnknownKeys = true }

    private fun decode(raw: String) = json.decodeFromString(AgentDefinitionEntity.serializer(), raw)

    @Test
    fun `v129 adds voice and refetches only agents`() {
        assertEquals(128, MIGRATION_128_129.startVersion)
        assertEquals(129, MIGRATION_128_129.endVersion)
        val statements = mutableListOf<String>()
        MIGRATION_128_129.migrate(recording(statements))
        assertEquals(
            listOf(
                "ALTER TABLE `agents` ADD COLUMN `voice` TEXT",
                "UPDATE `electric_offsets` SET `handle` = '', `offset` = '-1', `is_live` = 0, " +
                    "`needs_refetch` = 1 WHERE `shape` = 'agents'",
            ),
            statements,
        )
    }

    @Test
    fun `the synced row decodes voice, absent or malformed = none of its own`() {
        val full = AgentVoiceLike(
            language = "en",
            clipId = "c-1",
            clipName = "sloppy.wav",
            transcript = "Hi, Sloppy here.",
            wakePhrase = "Hey Sloppy",
        )
        val wire = """{"id":"a1","voice":{"language":"en","clipId":"c-1","clipName":"sloppy.wav",""" +
            """"transcript":"Hi, Sloppy here.","wakePhrase":"Hey Sloppy"}}"""
        val row = decode(wire)
        assertEquals(full, row.agentVoice)
        // Room stores the TEXT; a reload of that TEXT reads the same voice.
        assertEquals(full, AgentVoiceLike.parse(row.voice))
        // The jsonb as a string (Electric's text form) reads the same.
        assertEquals(
            AgentVoiceLike(language = "de"),
            decode("""{"id":"a1","voice":"{\"language\":\"de\"}"}""").agentVoice,
        )
        assertEquals(AgentVoiceLike.NONE, decode("""{"id":"a1","voice":{}}""").agentVoice)
        assertEquals(AgentVoiceLike.NONE, decode("""{"id":"a1","voice":null}""").agentVoice)
        assertEquals(AgentVoiceLike.NONE, decode("""{"id":"a1"}""").agentVoice)
        assertEquals(AgentVoiceLike.NONE, AgentVoiceLike.parse("not json"))
    }

    @Test
    fun `agents update sends only the touched voice keys, blank clears`() {
        val set = updateAgentInput("a1", UpdateAgentPatch(voice = AgentVoicePatch(language = "de", wakePhrase = "Hey Sloppy")))
        assertEquals(
            Json.parseToJsonElement("""{"id":"a1","voice":{"language":"de","wakePhrase":"Hey Sloppy"}}"""),
            set,
        )
        val cleared = updateAgentInput("a1", UpdateAgentPatch(voice = AgentVoicePatch(transcript = "")))
        assertEquals(Json.parseToJsonElement("""{"id":"a1","voice":{"transcript":null}}"""), cleared)
        assertFalse("voice" in updateAgentInput("a1", UpdateAgentPatch(name = "x")))
    }

    @Test
    fun `the clip route's refusal reads its error body`() {
        assertEquals("Voice clips must be WAV files", uploadRouteErrorMessage("""{"error":"Voice clips must be WAV files"}"""))
        assertEquals(null, uploadRouteErrorMessage("""{"error":""}"""))
        assertEquals(null, uploadRouteErrorMessage("oops"))
    }
}
