package com.exponential.app.data.api

import kotlinx.serialization.json.Json
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * EXP-825: wire-format lock for the three non-resume `steer.startSession`
 * forms. `prompt` (the composer's text + image embeds) rides LAST and only
 * when set — the shared Json's explicitNulls=false drops the null, so the
 * server sees NO prompt rather than an empty one (which the Chat / Create
 * action builtins would refuse). `account` (EXP-792) follows the same rule.
 */
class SteerStartInputWireTest {

    // Mirrors HttpClientProvider's shared Json — the one TrpcClient encodes with.
    private val json = Json {
        ignoreUnknownKeys = true
        explicitNulls = false
        encodeDefaults = true
    }

    @Test
    fun `single-issue start serializes the prompt last`() {
        val encoded = json.encodeToString(
            StartSessionInput.serializer(),
            StartSessionInput(
                issueId = "i-1",
                deviceId = "d-1",
                model = "opus",
                agent = "claude",
                resume = true,
                prompt = "Focus on the parser\n\n![image](/api/attachments/a-1)",
            ),
        )
        assertEquals(
            """{"issueId":"i-1","deviceId":"d-1","model":"opus","agent":"claude","resume":true,""" +
                """"prompt":"Focus on the parser\n\n![image](/api/attachments/a-1)"}""",
            encoded,
        )
    }

    @Test
    fun `a null prompt is omitted on every form`() {
        val single = json.encodeToString(
            StartSessionInput.serializer(),
            StartSessionInput(issueId = "i-1", deviceId = "d-1"),
        )
        assertEquals("""{"issueId":"i-1","deviceId":"d-1"}""", single)
        assertFalse(single.contains("prompt"))

        val batch = json.encodeToString(
            StartBatchSessionInput.serializer(),
            StartBatchSessionInput(issueIds = listOf("i-1", "i-2"), deviceId = "d-1", agent = "codex"),
        )
        assertEquals("""{"issueIds":["i-1","i-2"],"deviceId":"d-1","agent":"codex"}""", batch)

        val action = json.encodeToString(
            StartActionSessionInput.serializer(),
            StartActionSessionInput(actionId = "a-1", deviceId = "d-1"),
        )
        assertEquals("""{"actionId":"a-1","deviceId":"d-1"}""", action)
    }

    @Test
    fun `batch and action forms carry the prompt after their own fields`() {
        val batch = json.encodeToString(
            StartBatchSessionInput.serializer(),
            StartBatchSessionInput(issueIds = listOf("i-1", "i-2"), deviceId = "d-1", prompt = "Ship it"),
        )
        assertEquals("""{"issueIds":["i-1","i-2"],"deviceId":"d-1","prompt":"Ship it"}""", batch)

        // The Chat builtin: its teamId, the optional repo input and the
        // REQUIRED prompt — never a `prompt` input (EXP-825).
        val chat = json.encodeToString(
            StartActionSessionInput.serializer(),
            StartActionSessionInput(
                actionId = "builtin:chat",
                deviceId = "d-1",
                teamId = "t-1",
                inputs = mapOf("repo" to "r-1"),
                account = "profile-2",
                prompt = "What is open on the mobile board?",
            ),
        )
        assertEquals(
            """{"actionId":"builtin:chat","deviceId":"d-1","teamId":"t-1","inputs":{"repo":"r-1"},""" +
                """"account":"profile-2","prompt":"What is open on the mobile board?"}""",
            chat,
        )
        assertTrue(chat.indexOf("\"prompt\"") > chat.indexOf("\"inputs\""))
    }
}
