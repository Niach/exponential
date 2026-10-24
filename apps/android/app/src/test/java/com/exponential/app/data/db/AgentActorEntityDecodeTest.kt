package com.exponential.app.data.db

import kotlinx.serialization.json.Json
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/**
 * #EXP-1383: the eight new synced columns decode off the shapes' snake_case
 * rows, and a row without them (an older server) still decodes with NULLs.
 */
class AgentActorEntityDecodeTest {

    // Mirrors HttpClientProvider's shared Json.
    private val json = Json {
        ignoreUnknownKeys = true
        explicitNulls = false
        encodeDefaults = true
    }

    private val stamps = """"created_at": "2026-10-24 10:00:00+00", "updated_at": "2026-10-24 10:00:00+00""""

    @Test
    fun `the agent actor and assignee columns decode`() {
        val agent = json.decodeFromString(
            AgentDefinitionEntity.serializer(),
            """{"id": "G1", "name": "Triage", "instructions_updated_at": "2026-10-24 11:00:00+00", $stamps}""",
        )
        assertEquals("2026-10-24 11:00:00+00", agent.instructionsUpdatedAt)
        val session = json.decodeFromString(
            SessionEntity.serializer(),
            """{"id": "s1", "agent_instructions_at": "2026-10-24 10:30:00+00", "started_by_agent_id": "G1", $stamps}""",
        )
        assertEquals("2026-10-24 10:30:00+00", session.agentInstructionsAt)
        assertEquals("G1", session.startedByAgentId)
        val issue = json.decodeFromString(
            IssueEntity.serializer(),
            """{"id": "i1", "board_id": "b1", "number": 12, "identifier": "APP-12", "title": "T",
               "status": "backlog", "priority": "none", "sort_order": 1, "assignee_id": null,
               "assignee_agent_id": "G1", "creator_agent_id": "G2", $stamps}""",
        )
        assertEquals("G1", issue.assigneeAgentId)
        assertEquals("G2", issue.creatorAgentId)
        assertNull(issue.assigneeId)
        val draft = json.decodeFromString(
            IssueDraftEntity.serializer(),
            """{"id": "d1", "assignee_agent_id": "G1", $stamps}""",
        )
        assertEquals("G1", draft.assigneeAgentId)
        val event = json.decodeFromString(
            IssueEventEntity.serializer(),
            """{"id": "e1", "issue_id": "i1", "team_id": "t1", "actor_user_id": "U1", "actor_agent_id": "G1",
               "type": "status_changed", "payload": {"to": "done"}, $stamps}""",
        )
        assertEquals("G1", event.actorAgentId)
        val comment = json.decodeFromString(
            CommentEntity.serializer(),
            """{"id": "c1", "issue_id": "i1", "team_id": "t1", "author_id": "U1", "author_agent_id": "G1",
               "source": "mcp", $stamps}""",
        )
        assertEquals("G1", comment.authorAgentId)
    }

    @Test
    fun `rows from an older server decode with nulls`() {
        val event = json.decodeFromString(
            IssueEventEntity.serializer(),
            """{"id": "e1", "issue_id": "i1", "team_id": "t1", "type": "created", $stamps}""",
        )
        assertNull(event.actorAgentId)
        val comment = json.decodeFromString(
            CommentEntity.serializer(),
            """{"id": "c1", "issue_id": "i1", "team_id": "t1", $stamps}""",
        )
        assertNull(comment.authorAgentId)
    }
}
