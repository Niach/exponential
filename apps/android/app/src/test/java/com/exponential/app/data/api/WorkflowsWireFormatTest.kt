package com.exponential.app.data.api

import com.exponential.app.domain.WorkflowLaunch
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
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
                gate = null,
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
                gate = null,
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
                gate = null,
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
                ),
                gate = "agent",
                startOn = "pr_open",
            ),
        )
        assertEquals(
            """{"id":"wf-1","launch":{"agent":"claude","model":"opus","subagentModel":"fable",""" +
                """"effort":"high","account":"profile-2","maxParallel":5},"gate":"agent",""" +
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
                gate = null,
                startOn = null,
            ),
        )
        assertEquals("""{"id":"wf-1","launch":{"maxParallel":3}}""", bare)
        assertFalse(bare.contains("subagentModel"))
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
