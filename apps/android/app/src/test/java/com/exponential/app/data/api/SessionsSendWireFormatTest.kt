package com.exponential.app.data.api

import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Test

/**
 * The `sessions.send` wire: EXP-1734 retired the Subagents pick, so a new
 * root never carries `subagentModel`.
 */
class SessionsSendWireFormatTest {

    // Mirrors HttpClientProvider's shared Json — the one TrpcClient encodes with.
    private val json = Json {
        ignoreUnknownKeys = true
        explicitNulls = false
        encodeDefaults = true
    }

    @Test
    fun `a root carries no subagent model`() {
        val input = SessionsSendInput.root(
            teamId = "t-1",
            deviceId = "d-1",
            text = "Go",
            cli = "claude",
            model = "opus",
        )
        val encoded = json.encodeToString(input)
        assertEquals(
            """{"teamId":"t-1","deviceId":"d-1","cli":"claude","model":"opus","text":"Go"}""",
            encoded,
        )
        assertFalse("subagentModel" in encoded)
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
