package com.exponential.app.data.api

import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Test

/**
 * EXP-1583: a new root carries the composer's Subagents pick as
 * `subagentModel`; a blank one stays off the wire.
 */
class SessionsSendWireFormatTest {

    // Mirrors HttpClientProvider's shared Json — the one TrpcClient encodes with.
    private val json = Json {
        ignoreUnknownKeys = true
        explicitNulls = false
        encodeDefaults = true
    }

    @Test
    fun `a root carries the subagent model`() {
        val input = SessionsSendInput.root(
            teamId = "t-1",
            deviceId = "d-1",
            text = "Go",
            cli = "claude",
            model = "fable",
            subagentModel = "opus",
        )
        val encoded = json.encodeToString(input)
        assertEquals(
            """{"teamId":"t-1","deviceId":"d-1","cli":"claude","model":"fable","subagentModel":"opus","text":"Go"}""",
            encoded,
        )
    }

    @Test
    fun `a blank subagent model is omitted`() {
        val input = SessionsSendInput.root(teamId = "t-1", deviceId = "d-1", text = "Go", subagentModel = "")
        assertFalse("subagentModel" in json.encodeToString(input))
    }

    @Test
    fun `the Assistant's teams pick rides every send, null = every team (EXP-1715)`() {
        assertEquals(
            """{"text":"Hi","agentId":"a-1","scopeTeamIds":null}""",
            json.encodeToString(SessionsSendInput.toAgent("a-1", "Hi", scopeTeamIds = SessionsSendInput.scope(null))),
        )
        assertEquals(
            """{"sessionId":"s-1","text":"Hi","scopeTeamIds":["t1","t2"]}""",
            json.encodeToString(SessionsSendInput.message("s-1", "Hi", scopeTeamIds = SessionsSendInput.scope(listOf("t1", "t2")))),
        )
        // Another agent: never on the wire.
        assertFalse("scopeTeamIds" in json.encodeToString(SessionsSendInput.toAgent("a-2", "Hi")))
        assertFalse("scopeTeamIds" in json.encodeToString(SessionsSendInput.message("s-2", "Hi")))
    }
}
