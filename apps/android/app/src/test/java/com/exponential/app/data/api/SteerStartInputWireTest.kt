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

    /** SLOP-3: no start form carries a `workflowId` or a `stack` any more. */
    @Test
    fun `no start form carries a workflow id or a stack`() {
        val action = json.encodeToString(
            StartActionSessionInput.serializer(),
            StartActionSessionInput(actionId = "a-1", deviceId = "d-1"),
        )
        assertFalse(action.contains("workflowId"))
        val single = json.encodeToString(
            StartSessionInput.serializer(),
            StartSessionInput(issueId = "i-1", deviceId = "d-1"),
        )
        assertFalse(single.contains("stack"))
    }

    /**
     * EXP-981: `subagentModel` is claude-only and OMITTED for the CLI default
     * — the server validates it against the closed `codingModel` vocabulary,
     * which has no entry for the empty string.
     */
    @Test
    fun `the subagent model rides every start form and only when picked`() {
        val single = json.encodeToString(
            StartSessionInput.serializer(),
            StartSessionInput(
                issueId = "i-1",
                deviceId = "d-1",
                model = "opus",
                subagentModel = "fable",
                agent = "claude",
            ),
        )
        assertEquals(
            """{"issueId":"i-1","deviceId":"d-1","model":"opus","subagentModel":"fable",""" +
                """"agent":"claude"}""",
            single,
        )
        val batch = json.encodeToString(
            StartBatchSessionInput.serializer(),
            StartBatchSessionInput(
                issueIds = listOf("i-1", "i-2"),
                deviceId = "d-1",
                subagentModel = "fable",
            ),
        )
        assertTrue(batch.contains(""""subagentModel":"fable""""))
        assertFalse(
            json.encodeToString(
                StartSessionInput.serializer(),
                StartSessionInput(issueId = "i-1", deviceId = "d-1"),
            ).contains("subagentModel"),
        )
    }

    // EXP-1249: the per-run computer use flag sits next to `mcpServerIds`,
    // both before the prompt; absent when null (the device default).
    @Test
    fun `mcp servers and computer use ride before the prompt`() {
        val encoded = json.encodeToString(
            StartSessionInput.serializer(),
            StartSessionInput(
                issueId = "i-1",
                deviceId = "d-1",
                mcpServerIds = listOf("m-1"),
                computerUse = true,
                prompt = "go",
            ),
        )
        assertEquals(
            """{"issueId":"i-1","deviceId":"d-1","mcpServerIds":["m-1"],"computerUse":true,"prompt":"go"}""",
            encoded,
        )
        val action = json.encodeToString(
            StartActionSessionInput.serializer(),
            StartActionSessionInput(actionId = "a-1", deviceId = "d-1", computerUse = false),
        )
        assertEquals("""{"actionId":"a-1","deviceId":"d-1","computerUse":false}""", action)
        assertFalse(
            json.encodeToString(StartBatchSessionInput.serializer(), StartBatchSessionInput(listOf("i"), "d"))
                .contains("computerUse"),
        )
    }

    /** EXP-1284: the thread a start joins rides before the prompt, every form. */
    @Test
    fun `a thread id rides on every non-resume form before the prompt`() {
        val single = json.encodeToString(
            StartSessionInput.serializer(),
            StartSessionInput(issueId = "i-1", deviceId = "d-1", threadId = "t-1", prompt = "Go"),
        )
        assertEquals("""{"issueId":"i-1","deviceId":"d-1","threadId":"t-1","prompt":"Go"}""", single)
        val batch = json.encodeToString(
            StartBatchSessionInput.serializer(),
            StartBatchSessionInput(issueIds = listOf("i-1", "i-2"), deviceId = "d-1", threadId = "t-1"),
        )
        assertEquals("""{"issueIds":["i-1","i-2"],"deviceId":"d-1","threadId":"t-1"}""", batch)
        val action = json.encodeToString(
            StartActionSessionInput.serializer(),
            StartActionSessionInput(actionId = "builtin:chat", deviceId = "d-1", teamId = "team-1", threadId = "t-1", prompt = "Hi"),
        )
        assertEquals(
            """{"actionId":"builtin:chat","deviceId":"d-1","teamId":"team-1","threadId":"t-1","prompt":"Hi"}""",
            action,
        )
        // Omitted when null (the default placement).
        assertFalse(json.encodeToString(StartSessionInput.serializer(), StartSessionInput(issueId = "i-1", deviceId = "d-1")).contains("threadId"))
    }

    @Test
    fun `issues create carries the thread when filed into one`() {
        val encoded = json.encodeToString(
            CreateIssueInput.serializer(),
            CreateIssueInput(boardId = "b-1", title = "Bug", threadId = "t-1"),
        )
        assertTrue(encoded.contains(""""threadId":"t-1""""))
        assertFalse(json.encodeToString(CreateIssueInput.serializer(), CreateIssueInput(boardId = "b-1", title = "Bug")).contains("threadId"))
    }

    @Test
    fun `a resume carries its startId only when set`() {
        val pressed = json.encodeToString(
            ResumeSessionInput.serializer(),
            ResumeSessionInput(resumeSessionId = "s-1", deviceId = "d-1", startId = "start-1"),
        )
        assertEquals("""{"resumeSessionId":"s-1","deviceId":"d-1","startId":"start-1"}""", pressed)
        val plain = json.encodeToString(
            ResumeSessionInput.serializer(),
            ResumeSessionInput(resumeSessionId = "s-1", deviceId = "d-1"),
        )
        assertEquals("""{"resumeSessionId":"s-1","deviceId":"d-1"}""", plain)
    }
}
