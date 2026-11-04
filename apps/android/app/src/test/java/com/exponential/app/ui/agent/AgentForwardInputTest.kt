package com.exponential.app.ui.agent

import com.exponential.app.data.api.SessionsSendInput
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json
import org.junit.Assert.assertEquals
import org.junit.Test

/** EXP-1686 (chats.json AGENT CHAT): Start coding with an agent picked. */
class AgentForwardInputTest {
    // Mirrors HttpClientProvider's shared Json, the one TrpcClient encodes with.
    private val json = Json {
        ignoreUnknownKeys = true
        explicitNulls = false
        encodeDefaults = true
    }

    private fun wire(input: SessionsSendInput) = json.encodeToString(input)

    private val identifiers = mapOf("i-1" to "EXP-12", "i-2" to "EXP-13")

    @Test
    fun `a bare forward sends the refs in chip order`() {
        val input = agentForwardInput("a-1", null, emptyList(), listOf("i-2", "i-1"), identifiers)
        assertEquals("#EXP-13 #EXP-12", input.text)
        assertEquals(listOf("i-2", "i-1"), input.issueIds)
    }

    @Test
    fun `the refs lead the person's text`() {
        val input = agentForwardInput("a-1", "Please look.", emptyList(), listOf("i-1"), identifiers)
        assertEquals("#EXP-12\n\nPlease look.", input.text)
    }

    @Test
    fun `an empty text is omitted from the wire, never blank`() {
        // The issue has not synced to this install: nothing to write, the
        // server's `issueIds` still reach the agent.
        val input = agentForwardInput("a-1", null, emptyList(), listOf("i-9"), identifiers)
        assertEquals("""{"issueIds":["i-9"],"agentId":"a-1"}""", wire(input))
        assertEquals("""{"agentId":"a-1"}""", wire(SessionsSendInput.toAgent("a-1", "")))
    }

    @Test
    fun `no issues keep the text untouched`() {
        assertEquals("Hi", agentForwardInput("a-1", "Hi", emptyList(), null, identifiers).text)
    }
}
