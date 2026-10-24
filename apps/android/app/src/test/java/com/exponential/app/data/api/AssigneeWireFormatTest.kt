package com.exponential.app.data.api

import com.exponential.app.domain.AgentSettings
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.jsonObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * #EXP-1383: an issue's assignee is ONE of two exclusive columns, and every
 * write sends BOTH, the cleared one as an explicit JSON null (the shared
 * Json's explicitNulls=false would drop a null property).
 */
class AssigneeWireFormatTest {

    // Mirrors HttpClientProvider's shared Json — the one TrpcClient encodes with.
    private val json = Json {
        ignoreUnknownKeys = true
        explicitNulls = false
        encodeDefaults = true
    }

    @Test
    fun `issues update sends both columns`() {
        assertEquals(
            """{"id":"i1","assigneeId":null,"assigneeAgentId":"G1"}""",
            assigneeUpdateInput("i1", AgentSettings.Assignee(null, "G1")).toString(),
        )
        assertEquals(
            """{"id":"i1","assigneeId":"U1","assigneeAgentId":null}""",
            assigneeUpdateInput("i1", AgentSettings.Assignee("U1", null)).toString(),
        )
        assertEquals(
            """{"id":"i1","assigneeId":null,"assigneeAgentId":null}""",
            assigneeUpdateInput("i1", AgentSettings.Assignee.NONE).toString(),
        )
        // An agent wins and clears the person.
        assertEquals(
            """{"id":"i1","assigneeId":null,"assigneeAgentId":"G1"}""",
            assigneeUpdateInput("i1", AgentSettings.Assignee("U1", "G1")).toString(),
        )
    }

    @Test
    fun `a draft upsert always carries the agent column`() {
        val base = UpsertIssueDraftInput(id = "d1", teamId = "t1", boardId = "b1", title = "T", description = "")
        val cleared = json.encodeToJsonElement(UpsertIssueDraftInput.serializer(), base).jsonObject
        assertTrue("assigneeAgentId" in cleared)
        assertEquals(JsonNull, cleared["assigneeAgentId"])
        val agent = json.encodeToJsonElement(
            UpsertIssueDraftInput.serializer(),
            base.copy(assigneeAgentId = JsonPrimitive("G1")),
        ).jsonObject
        assertEquals(JsonPrimitive("G1"), agent["assigneeAgentId"])
        assertTrue("assigneeId" !in agent)
    }

    @Test
    fun `an issue create carries an agent assignee`() {
        val input = json.encodeToJsonElement(
            CreateIssueInput.serializer(),
            CreateIssueInput(boardId = "b1", title = "T", assigneeAgentId = "G1"),
        ).jsonObject
        assertEquals(JsonPrimitive("G1"), input["assigneeAgentId"])
        assertTrue("assigneeId" !in input)
    }
}
