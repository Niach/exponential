package com.exponential.app.data.api

import com.exponential.app.domain.WorkflowLaunch
import com.exponential.app.domain.workflowLaunch
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * EXP-981: wire-format lock for the `workflows` router's hand-built patches.
 * The router applies a key only when it is `!== undefined`, so an OMITTED key
 * means "keep" and only `deviceId` ever rides as an explicit null (the
 * "unbind"). Inside `launch`, an empty value is OMITTED: the server validates
 * model/effort/subagentModel against closed vocabularies that have no entry
 * for the empty string.
 */
class WorkflowsWireFormatTest {

    // Mirrors HttpClientProvider's shared Json.
    private val json = Json {
        ignoreUnknownKeys = true
        explicitNulls = false
        encodeDefaults = true
    }

    private fun encode(input: JsonObject) = json.encodeToString(JsonObject.serializer(), input)

    @Test
    fun `a rename carries only the name`() {
        val encoded = encode(
            updateWorkflowInput(
                id = "wf-1",
                name = "Mobile parity",
                deviceId = null,
                clearDevice = false,
                launch = null,
                startOn = null,
            ),
        )
        assertEquals("""{"id":"wf-1","name":"Mobile parity"}""", encoded)
    }

    @Test
    fun `binding and unbinding the runner are two different payloads`() {
        val bind = encode(
            updateWorkflowInput(
                id = "wf-1",
                name = null,
                deviceId = "dev-1",
                clearDevice = false,
                launch = null,
                startOn = null,
            ),
        )
        assertEquals("""{"id":"wf-1","deviceId":"dev-1"}""", bind)

        val unbind = encode(
            updateWorkflowInput(
                id = "wf-1",
                name = null,
                deviceId = null,
                clearDevice = true,
                launch = null,
                startOn = null,
            ),
        )
        assertEquals("""{"id":"wf-1","deviceId":null}""", unbind)
    }

    @Test
    fun `launch omits every empty option and always states max parallel`() {
        val full = encode(
            updateWorkflowInput(
                id = "wf-1",
                name = null,
                deviceId = null,
                clearDevice = false,
                launch = WorkflowLaunch(
                    agent = "claude",
                    model = "opus",
                    subagentModel = "fable",
                    effort = "high",
                    account = "profile-2",
                    maxParallel = 5,
                    reviewModel = "fable",
                ),
                startOn = "pr_open",
            ),
        )
        assertEquals(
            """{"id":"wf-1","launch":{"agent":"claude","model":"opus","subagentModel":"fable",""" +
                """"effort":"high","account":"profile-2","maxParallel":5,"reviewModel":"fable",""" +
                """"contractModel":null,"integrationModel":null,"riskModel":null},""" +
                """"startOn":"pr_open"}""",
            full,
        )

        val bare = encode(
            updateWorkflowInput(
                id = "wf-1",
                name = null,
                deviceId = null,
                clearDevice = false,
                launch = WorkflowLaunch(maxParallel = 3),
                startOn = null,
            ),
        )
        assertEquals(
            """{"id":"wf-1","launch":{"maxParallel":3,"contractModel":null,""" +
                """"integrationModel":null,"riskModel":null}}""",
            bare,
        )
        assertFalse(bare.contains("subagentModel"))
        // EXP-984: "" means "the engine picks the review model", which is the
        // ABSENT key — the server checks it against the model vocabulary.
        assertFalse(bare.contains("reviewModel"))
    }

    /**
     * EXP-1002: the router replaces the WHOLE `launch`, and for the three phase
     * pins it reads absent = keep, null = clear, string = set. This client
     * always states all three, so a phone edit of another option carries the
     * pins web/desktop set, and "no pin" arrives as an explicit null.
     */
    @Test
    fun `launch always states the three phase models, null when unpinned`() {
        val stored = workflowLaunch(
            """{"agent":"claude","model":"opus","maxParallel":2,""" +
                """"contractModel":"fable","riskModel":"opus","someFutureKey":1}""",
        )
        // The edit the phone makes: one OTHER option.
        val edited = stored.copy(effort = "high", maxParallel = 4)
        assertEquals("fable", edited.contractModel)
        assertNull(edited.integrationModel)
        assertEquals("opus", edited.riskModel)

        val patch = updateWorkflowInput(
            id = "wf-1",
            name = null,
            deviceId = null,
            clearDevice = false,
            launch = edited,
            startOn = null,
        )
        val launch = patch["launch"] as JsonObject
        assertEquals(JsonPrimitive("fable"), launch["contractModel"])
        assertTrue(launch.containsKey("integrationModel"))
        assertEquals(JsonNull, launch["integrationModel"])
        assertEquals(JsonPrimitive("opus"), launch["riskModel"])

        // An agent switch drops the pins (they are the OLD agent's models) —
        // as explicit nulls, never as absent keys the server would "keep".
        val switched = updateWorkflowInput(
            id = "wf-1",
            name = null,
            deviceId = null,
            clearDevice = false,
            launch = edited.copy(agent = "codex", model = "", effort = "").withoutPhaseModels(),
            startOn = null,
        )["launch"] as JsonObject
        for (key in listOf("contractModel", "integrationModel", "riskModel")) {
            assertEquals(JsonNull, switched[key])
        }
    }

    @Test
    fun `launch decodes with and without the phase models`() {
        val without = workflowLaunch("""{"agent":"claude","model":"opus"}""")
        assertNull(without.contractModel)
        assertNull(without.integrationModel)
        assertNull(without.riskModel)

        val with = workflowLaunch(
            """{"contractModel":"fable","integrationModel":null,"riskModel":""}""",
        )
        assertEquals("fable", with.contractModel)
        assertNull(with.integrationModel)
        assertNull(with.riskModel)
    }

    @Test
    fun `a node patch addresses its issue and omits what it does not set`() {
        val risk = encode(
            updateWorkflowNodeInput(
                workflowId = "wf-1",
                issueId = "issue-1",
                kind = null,
                risk = "high",
                touches = null,
            ),
        )
        assertEquals("""{"workflowId":"wf-1","issueId":"issue-1","risk":"high"}""", risk)

        val plan = encode(
            updateWorkflowNodeInput(
                workflowId = "wf-1",
                issueId = "issue-1",
                kind = "contract",
                risk = null,
                touches = listOf("apps/web/**", "packages/ui/**"),
            ),
        )
        assertEquals(
            """{"workflowId":"wf-1","issueId":"issue-1","kind":"contract",""" +
                """"touches":["apps/web/**","packages/ui/**"]}""",
            plan,
        )
        assertTrue(plan.indexOf("\"touches\"") > plan.indexOf("\"kind\""))
    }

    // ── Review gate, dynamic graphs (EXP-984) ───────────────────────────────

    @Test
    fun `admitting and dismissing a proposal differ only in the boolean`() {
        assertEquals(
            """{"nodeId":"node-1","admit":true}""",
            json.encodeToString(
                AdmitNodeInput.serializer(),
                AdmitNodeInput(nodeId = "node-1", admit = true),
            ),
        )
        assertEquals(
            """{"nodeId":"node-1","admit":false}""",
            json.encodeToString(
                AdmitNodeInput.serializer(),
                AdmitNodeInput(nodeId = "node-1", admit = false),
            ),
        )
    }

    // ── Running a workflow (EXP-982) ────────────────────────────────────────

    @Test
    fun `the run verbs carry the workflow id alone`() {
        // start / pause / resume / cancel share ONE input; the server's whole
        // judgement is made from the row it loads.
        assertEquals(
            """{"id":"wf-1"}""",
            json.encodeToString(WorkflowIdInput.serializer(), WorkflowIdInput(id = "wf-1")),
        )
    }

    @Test
    fun `approving and withdrawing differ only in the boolean`() {
        assertEquals(
            """{"nodeId":"node-1","approved":true}""",
            json.encodeToString(
                ApproveNodeInput.serializer(),
                ApproveNodeInput(nodeId = "node-1", approved = true),
            ),
        )
        assertEquals(
            """{"nodeId":"node-1","approved":false}""",
            json.encodeToString(
                ApproveNodeInput.serializer(),
                ApproveNodeInput(nodeId = "node-1", approved = false),
            ),
        )
    }

    @Test
    fun `resolving a node names retry or skip and nothing else`() {
        assertEquals(
            """{"nodeId":"node-1","action":"retry"}""",
            json.encodeToString(
                ResolveNodeInput.serializer(),
                ResolveNodeInput(nodeId = "node-1", action = WorkflowsApi.NODE_RETRY),
            ),
        )
        assertEquals(
            """{"nodeId":"node-1","action":"skip"}""",
            json.encodeToString(
                ResolveNodeInput.serializer(),
                ResolveNodeInput(nodeId = "node-1", action = WorkflowsApi.NODE_SKIP),
            ),
        )
    }
}
